//! Áudio: efeitos sonoros e música (.wav, .ogg, .mp3) via rodio.

use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player};
use std::collections::HashMap;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;

pub struct Audio {
    project_dir: PathBuf,
    sink: Option<MixerDeviceSink>,
    tried: bool,
    files: HashMap<String, Arc<[u8]>>,
    playing: Vec<Player>,
    music: Option<Player>,
    pub volume: f32,
    /// Quantos sons foram disparados (útil para testes e depuração).
    pub played: usize,
}

impl Audio {
    pub fn new(project_dir: PathBuf) -> Self {
        Audio {
            project_dir,
            sink: None,
            tried: false,
            files: HashMap::new(),
            playing: vec![],
            music: None,
            volume: 1.0,
            played: 0,
        }
    }

    /// Abre a saída de som na primeira vez que for necessária.
    fn open(&mut self) -> Result<(), String> {
        if self.sink.is_none() && !self.tried {
            self.tried = true;
            match DeviceSinkBuilder::open_default_sink() {
                Ok(mut s) => {
                    s.log_on_drop(false);
                    self.sink = Some(s);
                }
                Err(e) => return Err(format!("saída de áudio indisponível ({e}); o jogo segue sem som")),
            }
        }
        Ok(())
    }

    fn bytes(&mut self, rel: &str) -> Result<Arc<[u8]>, String> {
        if let Some(b) = self.files.get(rel) {
            return Ok(b.clone());
        }
        let data = std::fs::read(self.project_dir.join(rel))
            .map_err(|e| format!("não foi possível ler o som `{rel}` ({e})"))?;
        let data: Arc<[u8]> = Arc::from(data);
        self.files.insert(rel.to_string(), data.clone());
        Ok(data)
    }

    /// Toca um efeito sonoro uma vez. Devolve um aviso (não fatal) se não houver saída de som.
    pub fn play(&mut self, rel: &str, volume: f32) -> Result<Option<String>, String> {
        let data = self.bytes(rel)?;
        self.played += 1;
        let warn = self.open().err();
        let Some(sink) = &self.sink else { return Ok(warn) };
        let source = Decoder::new(Cursor::new(data)).map_err(|e| format!("som `{rel}` inválido ({e})"))?;
        let player = Player::connect_new(sink.mixer());
        player.set_volume((volume * self.volume).clamp(0.0, 4.0));
        player.append(source);
        self.playing.retain(|p| !p.empty());
        if self.playing.len() > 48 {
            self.playing.remove(0);
        }
        self.playing.push(player);
        Ok(warn)
    }

    /// Toca uma música em repetição (substitui a anterior).
    pub fn music(&mut self, rel: &str, volume: f32) -> Result<Option<String>, String> {
        let data = self.bytes(rel)?;
        self.played += 1;
        let warn = self.open().err();
        self.music = None;
        let Some(sink) = &self.sink else { return Ok(warn) };
        let source =
            Decoder::new_looped(Cursor::new(data)).map_err(|e| format!("música `{rel}` inválida ({e})"))?;
        let player = Player::connect_new(sink.mixer());
        player.set_volume((volume * self.volume).clamp(0.0, 4.0));
        player.append(source);
        self.music = Some(player);
        Ok(warn)
    }

    pub fn stop_music(&mut self) {
        self.music = None;
    }

    pub fn stop_all(&mut self) {
        self.music = None;
        self.playing.clear();
    }

    pub fn set_paused(&mut self, paused: bool) {
        for p in self.playing.iter().chain(self.music.iter()) {
            if paused {
                p.pause();
            } else {
                p.play();
            }
        }
    }
}

/// Gera um arquivo WAV (mono, 16 bits) a partir de uma função de onda — usado nos sons de exemplo.
pub fn synth_wav(seconds: f32, f: impl Fn(f32) -> f32) -> Vec<u8> {
    let rate = 44_100u32;
    let n = (seconds * rate as f32) as u32;
    let mut out = Vec::with_capacity(44 + n as usize * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + n * 2).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(n * 2).to_le_bytes());
    for i in 0..n {
        let t = i as f32 / rate as f32;
        let s = (f(t).clamp(-1.0, 1.0) * 30_000.0) as i16;
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_gerado_e_decodificavel() {
        let wav = synth_wav(0.1, |t| (t * 440.0 * std::f32::consts::TAU).sin());
        assert_eq!(&wav[..4], b"RIFF");
        let dec = Decoder::new(Cursor::new(wav)).expect("WAV válido");
        assert_eq!(dec.count(), 4410);
    }
}
