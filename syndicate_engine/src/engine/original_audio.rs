//! Runtime-local original sound/music catalog support.
//!
//! FreeSynd loads `SOUND-0/1.TAB` plus `.DAT` sample streams and XMI music at
//! runtime. This module mirrors the aggregate catalog rules without emitting or
//! storing asset bytes in generated source, docs, reports, or tests.

use std::{collections::BTreeMap, fs, path::Path};

use macroquad::audio::{PlaySoundParams, Sound, load_sound_from_bytes, play_sound};

const SOUND_TAB_START_OFFSET: usize = 58;
const SOUND_TAB_RECORD_BYTES: usize = 32;
const SOUND_BOGUS_SAMPLE_MAX_BYTES: usize = 144;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OriginalAudioCatalog {
    pub sound_sets: Vec<OriginalSoundSetSummary>,
    pub game_sample_candidates: usize,
    pub intro_sample_candidates: usize,
    pub music: OriginalMusicCatalog,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginalSoundSetSummary {
    pub label: String,
    pub sample_candidates: usize,
    pub skipped_bogus_samples: usize,
    pub truncated_samples: usize,
    pub dat_bytes_accounted: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OriginalMusicCatalog {
    pub syngame_xmi_available: bool,
    pub intro_xmi_available: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum OriginalAudioSampleKey {
    Pistol,
    Uzi,
    Door,
    Menu,
    MissionComplete,
}

#[derive(Debug, Clone)]
pub struct OriginalAudioSampleBank {
    sounds: BTreeMap<OriginalAudioSampleKey, Sound>,
    attempted_samples: usize,
    loaded_samples: usize,
    missing_samples: usize,
    volume: f32,
    muted: bool,
    music_attempted: bool,
    music_decoder_accepted: bool,
    music_blocker: Option<String>,
}

impl OriginalAudioCatalog {
    pub fn from_root(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref();
        let mut catalog = Self::default();
        for (set_label, tab_name, dat_name, intro) in [
            ("game SOUND-0", "SOUND-0.TAB", "SOUND-0.DAT", false),
            ("game SOUND-1", "SOUND-1.TAB", "SOUND-1.DAT", false),
            ("intro SOUND-2", "SOUND-2.TAB", "SOUND-2.DAT", true),
            ("intro SOUND-3", "SOUND-3.TAB", "SOUND-3.DAT", true),
        ] {
            if let Some(summary) = summarize_sound_set(root, set_label, tab_name, dat_name) {
                if intro {
                    catalog.intro_sample_candidates += summary.sample_candidates;
                } else {
                    catalog.game_sample_candidates += summary.sample_candidates;
                }
                catalog.sound_sets.push(summary);
            }
        }
        catalog.music = OriginalMusicCatalog {
            syngame_xmi_available: first_existing(root, "SYNGAME.XMI").is_some(),
            intro_xmi_available: first_existing(root, "INTRO.XMI").is_some(),
        };
        catalog
    }

    pub fn status_label(&self) -> String {
        let music = if self.music.syngame_xmi_available {
            "SYNGAME.XMI present"
        } else {
            "SYNGAME.XMI missing"
        };
        format!(
            "original audio catalog: game samples {}, intro samples {}, {}; playback mixer/XMI sequencing still gated",
            self.game_sample_candidates, self.intro_sample_candidates, music
        )
    }

    pub fn event_status_label(&self, event_label: &str) -> String {
        if self.game_sample_candidates == 0 {
            return format!("{event_label}: original sound samples unavailable; local event only");
        }
        let class = if event_label.contains("shot") || event_label.contains("weapon") {
            "weapon sample candidate"
        } else if event_label.contains("impact") || event_label.contains("hit") {
            "impact sample candidate"
        } else if event_label.contains("vehicle") || event_label.contains("car") {
            "vehicle sample candidate"
        } else if event_label.contains("door") || event_label.contains("ui") {
            "UI/door sample candidate"
        } else {
            "sample candidate"
        };
        format!(
            "{event_label}: {class}; runtime sample playback active when loaded, XMI/full mixer semantics gated"
        )
    }
}

impl OriginalAudioSampleKey {
    fn sample_index(self) -> usize {
        match self {
            // FreeSynd's `InGameSample` enum is an index into the loaded game sample vector.
            Self::Pistol => 1,
            Self::Uzi => 7,
            Self::MissionComplete => 14,
            Self::Door => 16,
            Self::Menu => 20,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Pistol => "pistol",
            Self::Uzi => "uzi",
            Self::Door => "door",
            Self::Menu => "menu",
            Self::MissionComplete => "mission complete",
        }
    }

    pub fn for_event_label(label: &str) -> Option<Self> {
        let lower = label.to_ascii_lowercase();
        if lower.contains("mission complete") || lower.contains("objective down") {
            return Some(Self::MissionComplete);
        }
        if lower.contains("door") || lower.contains("open") {
            return Some(Self::Door);
        }
        if lower.contains("ui") || lower.contains("select") || lower.contains("reset") {
            return Some(Self::Menu);
        }
        if lower.contains("uzi") || lower.contains("machine") {
            return Some(Self::Uzi);
        }
        if lower.contains("shot") || lower.contains("fire") || lower.contains("weapon") {
            return Some(Self::Pistol);
        }
        None
    }
}

impl OriginalAudioSampleBank {
    pub async fn from_root(root: impl AsRef<Path>) -> Self {
        let game_samples = load_game_sound_samples(root.as_ref());
        let mut bank = Self::default();
        for key in [
            OriginalAudioSampleKey::Pistol,
            OriginalAudioSampleKey::Uzi,
            OriginalAudioSampleKey::Door,
            OriginalAudioSampleKey::Menu,
            OriginalAudioSampleKey::MissionComplete,
        ] {
            bank.attempted_samples += 1;
            let Some(sample) = game_samples.get(key.sample_index()) else {
                bank.missing_samples += 1;
                continue;
            };
            let Some(playback_bytes) = original_sample_playback_bytes(sample) else {
                bank.missing_samples += 1;
                continue;
            };
            match load_sound_from_bytes(&playback_bytes).await {
                Ok(sound) => {
                    bank.loaded_samples += 1;
                    bank.sounds.insert(key, sound);
                }
                Err(_) => {
                    bank.missing_samples += 1;
                }
            }
        }
        if let Some(path) = first_existing(root.as_ref(), "SYNGAME.XMI") {
            bank.music_attempted = true;
            match fs::read(path) {
                Ok(bytes) => {
                    bank.music_decoder_accepted = false;
                    let looks_like_xmi = bytes.starts_with(b"FORM")
                        || bytes.windows(4).take(32).any(|chunk| chunk == b"XDIR")
                        || bytes.windows(4).take(32).any(|chunk| chunk == b"XMID");
                    bank.music_blocker = Some(if looks_like_xmi {
                        "SYNGAME.XMI present and recognized as an XMI/MIDI-family stream; an XMI/MIDI sequencer is still needed"
                            .to_string()
                    } else {
                        "SYNGAME.XMI present but not handed to the sample decoder; XMI/MIDI sequencing is still needed"
                            .to_string()
                    });
                }
                Err(_) => {
                    bank.music_blocker =
                        Some("SYNGAME.XMI present but could not be read locally".to_string());
                }
            }
        } else {
            bank.music_blocker = Some("SYNGAME.XMI missing from local original assets".to_string());
        }
        bank
    }

    pub fn play_event(&self, event_label: &str) -> Option<OriginalAudioSampleKey> {
        let key = OriginalAudioSampleKey::for_event_label(event_label)?;
        let sound = self.sounds.get(&key)?;
        play_sound(
            sound,
            PlaySoundParams {
                looped: false,
                volume: if self.muted { 0.0 } else { self.volume },
            },
        );
        Some(key)
    }

    pub fn adjust_volume(&mut self, delta: f32) {
        self.volume = (self.volume + delta).clamp(0.0, 1.0);
        if self.volume > 0.0 {
            self.muted = false;
        }
    }

    pub fn toggle_mute(&mut self) {
        self.muted = !self.muted;
    }

    pub fn volume_label(&self) -> String {
        if self.muted {
            format!("muted (vol {:.0}%)", self.volume * 100.0)
        } else {
            format!("vol {:.0}%", self.volume * 100.0)
        }
    }

    pub fn status_label(&self) -> String {
        let loaded = self
            .sounds
            .keys()
            .map(|key| key.label())
            .collect::<Vec<_>>()
            .join("/");
        let loaded = if loaded.is_empty() {
            "none".to_string()
        } else {
            loaded
        };
        let music = if self.music_decoder_accepted {
            "SYNGAME.XMI decoder accepted; sequenced music playback still gated".to_string()
        } else if self.music_attempted {
            self.music_blocker
                .clone()
                .unwrap_or_else(|| "SYNGAME.XMI attempted; music playback still gated".to_string())
        } else {
            self.music_blocker
                .clone()
                .unwrap_or_else(|| "music/XMI playback still gated".to_string())
        };
        format!(
            "runtime sound playback samples {}/{} loaded ({loaded}), {}; {music}",
            self.loaded_samples,
            self.attempted_samples,
            self.volume_label()
        )
    }
}

impl Default for OriginalAudioSampleBank {
    fn default() -> Self {
        Self {
            sounds: BTreeMap::new(),
            attempted_samples: 0,
            loaded_samples: 0,
            missing_samples: 0,
            volume: 0.55,
            muted: false,
            music_attempted: false,
            music_decoder_accepted: false,
            music_blocker: None,
        }
    }
}

fn summarize_sound_set(
    root: &Path,
    label: &str,
    tab_name: &str,
    dat_name: &str,
) -> Option<OriginalSoundSetSummary> {
    let tab = fs::read(first_existing(root, tab_name)?).ok()?;
    let dat = fs::read(first_existing(root, dat_name)?).ok()?;
    if tab.len() <= SOUND_TAB_START_OFFSET {
        return None;
    }
    let mut offset = 0usize;
    let mut sample_candidates = 0usize;
    let mut skipped_bogus_samples = 0usize;
    let mut truncated_samples = 0usize;
    for record in tab[SOUND_TAB_START_OFFSET..].chunks_exact(SOUND_TAB_RECORD_BYTES) {
        let sound_size = u32::from_le_bytes([record[0], record[1], record[2], record[3]]) as usize;
        if sound_size == 0 {
            continue;
        }
        if sound_size <= SOUND_BOGUS_SAMPLE_MAX_BYTES {
            skipped_bogus_samples += 1;
            offset = offset.saturating_add(sound_size);
            continue;
        }
        if offset.saturating_add(sound_size) > dat.len() {
            truncated_samples += 1;
            break;
        }
        sample_candidates += 1;
        offset += sound_size;
    }
    Some(OriginalSoundSetSummary {
        label: label.to_string(),
        sample_candidates,
        skipped_bogus_samples,
        truncated_samples,
        dat_bytes_accounted: offset.min(dat.len()),
    })
}

fn load_game_sound_samples(root: &Path) -> Vec<Vec<u8>> {
    let mut samples = Vec::new();
    for (tab_name, dat_name) in [
        ("SOUND-0.TAB", "SOUND-0.DAT"),
        ("SOUND-1.TAB", "SOUND-1.DAT"),
    ] {
        load_sound_samples_from_set(root, tab_name, dat_name, &mut samples);
    }
    samples
}

fn load_sound_samples_from_set(
    root: &Path,
    tab_name: &str,
    dat_name: &str,
    samples: &mut Vec<Vec<u8>>,
) {
    let Some(tab_path) = first_existing(root, tab_name) else {
        return;
    };
    let Some(dat_path) = first_existing(root, dat_name) else {
        return;
    };
    let Ok(tab) = fs::read(tab_path) else {
        return;
    };
    let Ok(dat) = fs::read(dat_path) else {
        return;
    };
    if tab.len() <= SOUND_TAB_START_OFFSET {
        return;
    }
    let mut offset = 0usize;
    for record in tab[SOUND_TAB_START_OFFSET..].chunks_exact(SOUND_TAB_RECORD_BYTES) {
        let sound_size = u32::from_le_bytes([record[0], record[1], record[2], record[3]]) as usize;
        if sound_size == 0 {
            continue;
        }
        if sound_size > SOUND_BOGUS_SAMPLE_MAX_BYTES
            && offset.saturating_add(sound_size) <= dat.len()
        {
            let mut sample = dat[offset..offset + sound_size].to_vec();
            patch_known_freesynd_sample_rate(samples.len(), &mut sample);
            samples.push(sample);
        }
        offset = offset.saturating_add(sound_size);
    }
}

fn patch_known_freesynd_sample_rate(sample_index: usize, sample: &mut [u8]) {
    if sample.len() <= 0x1e {
        return;
    }
    match sample_index {
        // FreeSynd patches these exact loaded-sample ordinals before SDL_mixer decode.
        12 | 23 => sample[0x1e] = 0x9c,
        24 => sample[0x1e] = 0x38,
        _ => {}
    }
}

fn original_sample_playback_bytes(sample: &[u8]) -> Option<Vec<u8>> {
    if sample.starts_with(b"Creative Voice File") {
        voc_to_wav_bytes(sample)
    } else if sample.starts_with(b"RIFF") || sample.starts_with(b"OggS") {
        Some(sample.to_vec())
    } else {
        None
    }
}

fn voc_to_wav_bytes(voc: &[u8]) -> Option<Vec<u8>> {
    if voc.len() < 26 || !voc.starts_with(b"Creative Voice File") {
        return None;
    }
    let header_size = u16::from_le_bytes([voc[20], voc[21]]) as usize;
    if header_size >= voc.len() {
        return None;
    }
    let mut pos = header_size;
    let mut sample_rate = None;
    let mut pcm = Vec::new();
    while pos + 4 <= voc.len() {
        let block_type = voc[pos];
        if block_type == 0 {
            break;
        }
        let size = (voc[pos + 1] as usize)
            | ((voc[pos + 2] as usize) << 8)
            | ((voc[pos + 3] as usize) << 16);
        pos += 4;
        if pos.saturating_add(size) > voc.len() {
            break;
        }
        let block = &voc[pos..pos + size];
        match block_type {
            1 if block.len() >= 2 && block[1] == 0 => {
                let divisor = 256u32.saturating_sub(block[0] as u32).max(1);
                sample_rate = Some((1_000_000 / divisor).clamp(4_000, 44_100));
                pcm.extend_from_slice(&block[2..]);
            }
            2 if sample_rate.is_some() => {
                pcm.extend_from_slice(block);
            }
            9 if block.len() >= 12 => {
                let rate = u32::from_le_bytes([block[0], block[1], block[2], block[3]]);
                let bits = block[4];
                let channels = block[5].max(1);
                let codec = u16::from_le_bytes([block[6], block[7]]);
                if bits == 8 && channels == 1 && codec == 0 {
                    sample_rate = Some(rate.clamp(4_000, 44_100));
                    pcm.extend_from_slice(&block[12..]);
                }
            }
            _ => {}
        }
        pos += size;
    }
    let sample_rate = sample_rate?;
    if pcm.is_empty() {
        return None;
    }
    Some(unsigned_8bit_mono_wav(sample_rate, &pcm))
}

fn unsigned_8bit_mono_wav(sample_rate: u32, pcm: &[u8]) -> Vec<u8> {
    let data_len = pcm.len() as u32;
    let mut wav = Vec::with_capacity(44 + pcm.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&8u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    wav.extend_from_slice(pcm);
    wav
}

fn first_existing(root: &Path, file_name: &str) -> Option<std::path::PathBuf> {
    [
        file_name.to_string(),
        format!("SYNDICAT/DATA/{file_name}"),
        format!("DATADISK/DATA/{file_name}"),
        format!("SOUND/{file_name}"),
    ]
    .into_iter()
    .map(|relative| root.join(relative))
    .find(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, time::SystemTime};

    #[test]
    fn sound_catalog_counts_without_exposing_bytes() {
        let root = std::env::temp_dir().join(format!(
            "syndicate_audio_catalog_{}",
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let mut tab = vec![0u8; SOUND_TAB_START_OFFSET];
        tab.extend_from_slice(&(200u32.to_le_bytes()));
        tab.resize(tab.len() + SOUND_TAB_RECORD_BYTES - 4, 0);
        tab.extend_from_slice(&(64u32.to_le_bytes()));
        tab.resize(tab.len() + SOUND_TAB_RECORD_BYTES - 4, 0);
        fs::write(root.join("SOUND-0.TAB"), tab).unwrap();
        fs::write(root.join("SOUND-0.DAT"), vec![1u8; 264]).unwrap();
        fs::write(root.join("SYNGAME.XMI"), b"xmi").unwrap();

        let catalog = OriginalAudioCatalog::from_root(&root);
        assert_eq!(catalog.game_sample_candidates, 1);
        assert!(catalog.music.syngame_xmi_available);
        let label = catalog.status_label();
        assert!(label.contains("game samples 1"));
        assert!(!label.contains("01 01"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn sample_event_mapping_uses_freesynd_indices_without_bytes() {
        assert_eq!(
            OriginalAudioSampleKey::for_event_label("weapon shot Pistol"),
            Some(OriginalAudioSampleKey::Pistol)
        );
        assert_eq!(
            OriginalAudioSampleKey::for_event_label("door ui open"),
            Some(OriginalAudioSampleKey::Door)
        );
        assert_eq!(OriginalAudioSampleKey::Uzi.sample_index(), 7);
        assert!(
            !OriginalAudioSampleBank::default()
                .status_label()
                .contains("00 00")
        );
    }

    #[test]
    fn runtime_audio_volume_and_xmi_status_stay_asset_safe() {
        let mut bank = OriginalAudioSampleBank::default();
        assert_eq!(bank.volume_label(), "vol 55%");
        bank.adjust_volume(0.60);
        assert_eq!(bank.volume_label(), "vol 100%");
        bank.toggle_mute();
        assert_eq!(bank.volume_label(), "muted (vol 100%)");
        let label = bank.status_label();
        assert!(label.contains("muted"));
        assert!(label.contains("music/XMI playback still gated"));
        assert!(!label.contains("00 00"));
    }

    #[test]
    fn converts_synthetic_voc_pcm_to_wav_without_asset_bytes() {
        let mut voc = b"Creative Voice File\x1A".to_vec();
        voc.extend_from_slice(&26u16.to_le_bytes());
        voc.extend_from_slice(&0x0114u16.to_le_bytes());
        voc.extend_from_slice(&0u16.to_le_bytes());
        voc.push(1);
        voc.extend_from_slice(&5u32.to_le_bytes()[0..3]);
        voc.push(165);
        voc.push(0);
        voc.extend_from_slice(&[128, 140, 128]);
        voc.push(0);
        let wav = voc_to_wav_bytes(&voc).expect("synthetic VOC should convert");
        assert!(wav.starts_with(b"RIFF"));
        assert!(wav.windows(4).any(|chunk| chunk == b"data"));
        assert!(wav.len() < 64);
    }
}
