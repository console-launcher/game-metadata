// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A minimal MP4 muxer: one H.264 video track and optionally one audio track (AAC or Opus), with the `moov` box first so the
//! file plays while it downloads. It does not encode anything: the browser's WebCodecs encoders do, and hand over their config
//! (`avcC`, the AAC `AudioSpecificConfig`, the `OpusHead`) and one encoded chunk per sample (H.264 length-prefixed, i.e. the
//! `avc` format, not Annex B). No B-frames (decode order is display order), one chunk per track.

use crate::Error;

/// What the audio track holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Audio {
    /// AAC-LC: the `AudioSpecificConfig` bytes (WebCodecs' `description`).
    Aac {
        /// The `AudioSpecificConfig`.
        config: Vec<u8>,
    },
    /// Opus (48 kHz): fields of the `OpusHead`.
    Opus {
        /// Pre-skip in 48 kHz samples.
        pre_skip: u16,
        /// The rate of the original input, informational.
        input_rate: u32,
    },
}

/// One track's samples: the sizes, durations and key flags, and the bytes.
#[derive(Debug, Default)]
struct Samples {
    data: Vec<u8>,
    sizes: Vec<u32>,
    durations: Vec<u32>,
    keys: Vec<bool>,
}

impl Samples {
    fn push(&mut self, data: &[u8], duration: u32, key: bool) -> Result<(), Error> {
        if data.is_empty() || duration == 0 || u32::try_from(data.len()).is_err() {
            return Err(Error::BadFrame);
        }
        self.data.extend_from_slice(data);
        self.sizes.push(data.len() as u32);
        self.durations.push(duration);
        self.keys.push(key);
        Ok(())
    }

    fn ticks(&self) -> u64 {
        self.durations.iter().map(|&d| u64::from(d)).sum()
    }
}

#[derive(Debug)]
struct AudioTrack {
    audio: Audio,
    rate: u32,
    channels: u16,
    samples: Samples,
}

/// Writes an MP4.
#[derive(Debug)]
pub struct Mp4Writer {
    width: u16,
    height: u16,
    timescale: u32,
    avcc: Vec<u8>,
    video: Samples,
    audio: Option<AudioTrack>,
}

/// A box of `kind` around `payload`.
fn bx(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut b = Vec::with_capacity(payload.len() + 8);
    b.extend_from_slice(&(payload.len() as u32 + 8).to_be_bytes());
    b.extend_from_slice(kind);
    b.extend_from_slice(payload);
    b
}

/// A box with a version and 24 bits of flags first.
fn full(kind: &[u8; 4], flags: u32, payload: &[u8]) -> Vec<u8> {
    let mut p = flags.to_be_bytes().to_vec();
    p.extend_from_slice(payload);
    bx(kind, &p)
}

fn cat(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

const UNITY: [u32; 9] = [0x0001_0000, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000];

impl Mp4Writer {
    /// A writer of `width` x `height` video whose sample durations are in 1/`timescale` seconds; `avcc` is the
    /// `AVCDecoderConfigurationRecord`.
    pub fn new(width: u16, height: u16, timescale: u32, avcc: &[u8]) -> Result<Mp4Writer, Error> {
        if width == 0 || height == 0 || timescale == 0 {
            return Err(Error::BadSize);
        }
        if avcc.len() < 7 || avcc[0] != 1 || avcc.len() > 0xFFFF {
            return Err(Error::BadFrame);
        }
        Ok(Mp4Writer { width, height, timescale, avcc: avcc.to_vec(), video: Samples::default(), audio: None })
    }

    /// Adds the audio track: `rate` Hz, `channels`.
    pub fn set_audio(&mut self, audio: Audio, rate: u32, channels: u16) -> Result<(), Error> {
        let bad_config = matches!(&audio, Audio::Aac { config } if config.is_empty() || config.len() > 100);
        if bad_config || rate == 0 || channels == 0 {
            return Err(Error::BadFrame);
        }
        self.audio = Some(AudioTrack { audio, rate, channels, samples: Samples::default() });
        Ok(())
    }

    /// Adds a video sample (length-prefixed NAL units) lasting `duration` ticks.
    pub fn push_video(&mut self, data: &[u8], duration: u32, key: bool) -> Result<(), Error> {
        self.video.push(data, duration, key)
    }

    /// Adds an audio sample lasting `duration` samples of the audio rate (1024 for AAC, 960 for 20 ms of Opus).
    pub fn push_audio(&mut self, data: &[u8], duration: u32) -> Result<(), Error> {
        self.audio.as_mut().ok_or(Error::BadFrame)?.samples.push(data, duration, true)
    }

    /// The finished file.
    pub fn finish(self) -> Result<Vec<u8>, Error> {
        if self.video.sizes.is_empty() {
            return Err(Error::BadFrame);
        }
        let ftyp = bx(b"ftyp", &cat(&[b"isom", &512u32.to_be_bytes(), b"isomiso2avc1mp41"]));
        let mut mdat_payload = self.video.data.clone();
        let audio_at = mdat_payload.len();
        if let Some(a) = &self.audio {
            mdat_payload.extend_from_slice(&a.samples.data);
        }
        if mdat_payload.len() as u64 + 0x10000 > u64::from(u32::MAX) {
            return Err(Error::BadSize); // 32-bit chunk offsets
        }
        // The chunk offsets are 32-bit fields, so the moov has the same size whatever they hold: build it once to measure it.
        let moov_len = self.moov(0, 0).len();
        let first = (ftyp.len() + moov_len + 8) as u32;
        let moov = self.moov(first, first + audio_at as u32);
        let mut out = ftyp;
        out.extend_from_slice(&moov);
        out.extend_from_slice(&bx(b"mdat", &mdat_payload));
        Ok(out)
    }

    fn moov(&self, video_at: u32, audio_at: u32) -> Vec<u8> {
        let movie_ticks = |ticks: u64, scale: u32| ticks * 1000 / u64::from(scale);
        let mut duration = movie_ticks(self.video.ticks(), self.timescale);
        if let Some(a) = &self.audio {
            duration = duration.max(movie_ticks(a.samples.ticks(), a.rate));
        }
        let mut mvhd = Vec::new();
        for v in [0, 0, 1000, duration as u32, 0x0001_0000] {
            mvhd.extend_from_slice(&v.to_be_bytes());
        }
        mvhd.extend_from_slice(&[1, 0, 0, 0]); // volume 1.0, reserved
        mvhd.extend_from_slice(&[0; 8]);
        for m in UNITY {
            mvhd.extend_from_slice(&m.to_be_bytes());
        }
        mvhd.extend_from_slice(&[0; 24]);
        mvhd.extend_from_slice(&(if self.audio.is_some() { 3u32 } else { 2 }).to_be_bytes());
        let mut moov = full(b"mvhd", 0, &mvhd);
        let video_entry = {
            let mut e = vec![0; 6];
            e.extend_from_slice(&1u16.to_be_bytes());
            e.extend_from_slice(&[0; 16]);
            e.extend_from_slice(&self.width.to_be_bytes());
            e.extend_from_slice(&self.height.to_be_bytes());
            e.extend_from_slice(&0x0048_0000u32.to_be_bytes());
            e.extend_from_slice(&0x0048_0000u32.to_be_bytes());
            e.extend_from_slice(&[0; 4]);
            e.extend_from_slice(&1u16.to_be_bytes());
            e.extend_from_slice(&[0; 32]);
            e.extend_from_slice(&0x0018u16.to_be_bytes());
            e.extend_from_slice(&0xFFFFu16.to_be_bytes());
            e.extend_from_slice(&bx(b"avcC", &self.avcc));
            bx(b"avc1", &e)
        };
        moov.extend_from_slice(&bx(
            b"trak",
            &track(
                1,
                Some((self.width, self.height)),
                b"vide",
                self.timescale,
                &self.video,
                video_entry,
                video_at,
                &full(b"vmhd", 1, &[0; 8]),
                movie_ticks(self.video.ticks(), self.timescale) as u32,
            ),
        ));
        if let Some(a) = &self.audio {
            let mut e = vec![0; 6];
            e.extend_from_slice(&1u16.to_be_bytes());
            e.extend_from_slice(&[0; 8]);
            e.extend_from_slice(&a.channels.to_be_bytes());
            e.extend_from_slice(&16u16.to_be_bytes());
            e.extend_from_slice(&[0; 4]);
            e.extend_from_slice(&(a.rate.min(0xFFFF) << 16).to_be_bytes());
            let entry = match &a.audio {
                Audio::Aac { config } => {
                    e.extend_from_slice(&esds(config));
                    bx(b"mp4a", &e)
                }
                Audio::Opus { pre_skip, input_rate } => {
                    let mut d = vec![0, a.channels.min(255) as u8];
                    d.extend_from_slice(&pre_skip.to_be_bytes());
                    d.extend_from_slice(&input_rate.to_be_bytes());
                    d.extend_from_slice(&[0, 0, 0]); // output gain, channel mapping family 0
                    e.extend_from_slice(&bx(b"dOps", &d));
                    bx(b"Opus", &e)
                }
            };
            moov.extend_from_slice(&bx(
                b"trak",
                &track(
                    2,
                    None,
                    b"soun",
                    a.rate,
                    &a.samples,
                    entry,
                    audio_at,
                    &full(b"smhd", 0, &[0; 4]),
                    movie_ticks(a.samples.ticks(), a.rate) as u32,
                ),
            ));
        }
        bx(b"moov", &moov)
    }
}

/// The `esds` box of an AAC track.
fn esds(config: &[u8]) -> Vec<u8> {
    let decoder_specific = cat(&[&[5, config.len() as u8], config]);
    let mut config_d = vec![0x40, 0x15, 0, 0, 0];
    config_d.extend_from_slice(&0u32.to_be_bytes());
    config_d.extend_from_slice(&0u32.to_be_bytes());
    config_d.extend_from_slice(&decoder_specific);
    let config_d = cat(&[&[4, config_d.len() as u8], &config_d]);
    let es = cat(&[&[0, 1, 0], &config_d, &[6, 1, 2]]);
    full(b"esds", 0, &cat(&[&[3, es.len() as u8], &es]))
}

/// A `trak` box's content.
#[allow(clippy::too_many_arguments)]
fn track(
    id: u32,
    picture: Option<(u16, u16)>,
    handler: &[u8; 4],
    timescale: u32,
    samples: &Samples,
    entry: Vec<u8>,
    chunk_at: u32,
    media_header: &[u8],
    movie_duration: u32,
) -> Vec<u8> {
    let mut tkhd = Vec::new();
    for v in [0, 0, id, 0, movie_duration] {
        tkhd.extend_from_slice(&v.to_be_bytes());
    }
    tkhd.extend_from_slice(&[0; 8]);
    tkhd.extend_from_slice(&[0, 0, 0, 0]); // layer, alternate group
    tkhd.extend_from_slice(&(if picture.is_none() { 0x0100u16 } else { 0 }).to_be_bytes());
    tkhd.extend_from_slice(&[0, 0]);
    for m in UNITY {
        tkhd.extend_from_slice(&m.to_be_bytes());
    }
    let (w, h) = picture.unwrap_or((0, 0));
    tkhd.extend_from_slice(&(u32::from(w) << 16).to_be_bytes());
    tkhd.extend_from_slice(&(u32::from(h) << 16).to_be_bytes());
    let mut mdhd = Vec::new();
    for v in [0, 0, timescale, samples.ticks() as u32] {
        mdhd.extend_from_slice(&v.to_be_bytes());
    }
    mdhd.extend_from_slice(&0x55C4u16.to_be_bytes()); // "und"
    mdhd.extend_from_slice(&[0, 0]);
    let hdlr = full(b"hdlr", 0, &cat(&[&[0; 4], handler, &[0; 12], b"gamemeta\0"]));
    let dinf = bx(b"dinf", &full(b"dref", 0, &cat(&[&1u32.to_be_bytes(), &full(b"url ", 1, &[])])));
    // Sample tables.
    let mut stts = Vec::new();
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for &d in &samples.durations {
        match runs.last_mut() {
            Some((n, dur)) if *dur == d => *n += 1,
            _ => runs.push((1, d)),
        }
    }
    stts.extend_from_slice(&(runs.len() as u32).to_be_bytes());
    for (n, d) in runs {
        stts.extend_from_slice(&n.to_be_bytes());
        stts.extend_from_slice(&d.to_be_bytes());
    }
    let mut stsz = vec![0; 4];
    stsz.extend_from_slice(&(samples.sizes.len() as u32).to_be_bytes());
    for s in &samples.sizes {
        stsz.extend_from_slice(&s.to_be_bytes());
    }
    let count = samples.sizes.len() as u32;
    let stsc = cat(&[&1u32.to_be_bytes(), &1u32.to_be_bytes(), &count.to_be_bytes(), &1u32.to_be_bytes()]);
    let stco = cat(&[&1u32.to_be_bytes(), &chunk_at.to_be_bytes()]);
    let mut stbl = full(b"stsd", 0, &cat(&[&1u32.to_be_bytes(), &entry]));
    stbl.extend_from_slice(&full(b"stts", 0, &stts));
    if samples.keys.iter().any(|k| !k) {
        let keys: Vec<u32> = samples.keys.iter().enumerate().filter(|(_, k)| **k).map(|(i, _)| i as u32 + 1).collect();
        let mut stss = (keys.len() as u32).to_be_bytes().to_vec();
        for k in keys {
            stss.extend_from_slice(&k.to_be_bytes());
        }
        stbl.extend_from_slice(&full(b"stss", 0, &stss));
    }
    stbl.extend_from_slice(&full(b"stsc", 0, &stsc));
    stbl.extend_from_slice(&full(b"stsz", 0, &stsz));
    stbl.extend_from_slice(&full(b"stco", 0, &stco));
    let minf = cat(&[media_header, &dinf, &bx(b"stbl", &stbl)]);
    let mdia = cat(&[&full(b"mdhd", 0, &mdhd), &hdlr, &bx(b"minf", &minf)]);
    cat(&[&full(b"tkhd", 3, &tkhd), &bx(b"mdia", &mdia)])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A box: (type, payload).
    fn boxes(mut b: &[u8]) -> Vec<([u8; 4], &[u8])> {
        let mut out = vec![];
        while !b.is_empty() {
            let size = u32::from_be_bytes(b[..4].try_into().unwrap()) as usize;
            assert!(size >= 8 && size <= b.len(), "box size {size} of {}", b.len());
            out.push((b[4..8].try_into().unwrap(), &b[8..size]));
            b = &b[size..];
        }
        out
    }

    fn find<'a>(b: &'a [u8], path: &[&[u8; 4]]) -> &'a [u8] {
        let mut cur = b;
        for kind in path {
            cur = boxes(cur)
                .into_iter()
                .find(|(k, _)| k == *kind)
                .unwrap_or_else(|| panic!("no {}", String::from_utf8_lossy(*kind)))
                .1;
        }
        cur
    }

    fn be32(b: &[u8], at: usize) -> u32 {
        u32::from_be_bytes(b[at..at + 4].try_into().unwrap())
    }

    const AVCC: [u8; 11] = [1, 0x42, 0xC0, 0x1E, 0xFF, 0xE1, 0, 2, 0x67, 0x42, 0];

    /// (stts runs, stsz sizes, stss keys, chunk offset) of the track box `trak`.
    #[allow(clippy::type_complexity)]
    fn tables(trak: &[u8]) -> (Vec<(u32, u32)>, Vec<u32>, Option<Vec<u32>>, u32) {
        let stbl = find(trak, &[b"mdia", b"minf", b"stbl"]);
        let stts = find(stbl, &[b"stts"]);
        let runs = (0..be32(stts, 4) as usize).map(|i| (be32(stts, 8 + 8 * i), be32(stts, 12 + 8 * i))).collect();
        let stsz = find(stbl, &[b"stsz"]);
        let sizes = (0..be32(stsz, 8) as usize).map(|i| be32(stsz, 12 + 4 * i)).collect();
        let stss = boxes(stbl)
            .into_iter()
            .find(|b| &b.0 == b"stss")
            .map(|b| (0..be32(b.1, 4) as usize).map(|i| be32(b.1, 8 + 4 * i)).collect());
        let stco = find(stbl, &[b"stco"]);
        assert_eq!(be32(stco, 4), 1, "one chunk");
        (runs, sizes, stss, be32(stco, 8))
    }

    fn sample_file() -> (Vec<u8>, Vec<Vec<u8>>) {
        let mut w = Mp4Writer::new(400, 240, 60, &AVCC).unwrap();
        let samples: Vec<Vec<u8>> = (0..5u8).map(|i| vec![i + 1; 3 + usize::from(i)]).collect();
        for (i, s) in samples.iter().enumerate() {
            w.push_video(s, 1, i % 4 == 0).unwrap();
        }
        (w.finish().unwrap(), samples)
    }

    #[test]
    fn the_top_level_is_ftyp_then_moov_then_mdat() {
        let (file, _) = sample_file();
        let top = boxes(&file);
        let names: Vec<&[u8]> = top.iter().map(|b| &b.0[..]).collect();
        assert_eq!(names, [&b"ftyp"[..], b"moov", b"mdat"]);
        assert_eq!(&top[0].1[..4], b"isom");
    }

    #[test]
    fn the_video_track_describes_the_picture() {
        let (file, _) = sample_file();
        let moov = find(&file, &[b"moov"]);
        let mvhd = find(moov, &[b"mvhd"]);
        assert_eq!((be32(mvhd, 12), be32(mvhd, 16)), (1000, 5 * 1000 / 60), "movie timescale 1000; 5 ticks of 1/60 s");
        let trak = find(moov, &[b"trak"]);
        let tkhd = find(trak, &[b"tkhd"]);
        assert_eq!(tkhd[3], 3, "enabled and in the movie");
        assert_eq!((be32(tkhd, 76), be32(tkhd, 80)), (400 << 16, 240 << 16));
        assert_eq!(&find(trak, &[b"mdia", b"hdlr"])[8..12], b"vide");
        let mdhd = find(trak, &[b"mdia", b"mdhd"]);
        assert_eq!((be32(mdhd, 12), be32(mdhd, 16)), (60, 5));
        let stsd = find(trak, &[b"mdia", b"minf", b"stbl", b"stsd"]);
        assert_eq!(be32(stsd, 4), 1);
        let avc1 = &stsd[8..];
        assert_eq!(&avc1[4..8], b"avc1");
        assert_eq!((u16::from_be_bytes([avc1[32], avc1[33]]), u16::from_be_bytes([avc1[34], avc1[35]])), (400, 240));
        assert_eq!(boxes(&avc1[86..]), [(*b"avcC", &AVCC[..])], "the encoder's avcC, untouched");
    }

    #[test]
    fn the_sample_tables_match_and_the_chunk_offset_points_at_the_data() {
        let (file, samples) = sample_file();
        let trak = find(&file, &[b"moov", b"trak"]);
        let (runs, sizes, keys, offset) = tables(trak);
        assert_eq!(runs, [(5, 1)], "five samples of one tick, run-length coded");
        assert_eq!(sizes, [3, 4, 5, 6, 7]);
        assert_eq!(keys, Some(vec![1, 5]), "samples 1 and 5 are key frames (1-based)");
        let mdat = boxes(&file).into_iter().find(|b| &b.0 == b"mdat").unwrap();
        let mdat_start = file.len() - mdat.1.len();
        assert_eq!(offset as usize, mdat_start, "the chunk starts right after the mdat header");
        assert_eq!(&file[offset as usize..], samples.concat());
    }

    #[test]
    fn all_key_frames_need_no_sync_table() {
        let mut w = Mp4Writer::new(16, 16, 60, &AVCC).unwrap();
        w.push_video(&[1, 2, 3], 1, true).unwrap();
        w.push_video(&[4], 2, true).unwrap();
        let file = w.finish().unwrap();
        let (runs, _, keys, _) = tables(find(&file, &[b"moov", b"trak"]));
        assert_eq!((runs, keys), (vec![(1, 1), (1, 2)], None));
    }

    #[test]
    fn an_aac_track_follows_with_its_config() {
        let mut w = Mp4Writer::new(400, 240, 60, &AVCC).unwrap();
        w.set_audio(Audio::Aac { config: vec![0x12, 0x10] }, 44100, 2).unwrap();
        w.push_video(&[9; 4], 60, true).unwrap();
        for i in 0..3u8 {
            w.push_audio(&[i; 10], 1024).unwrap();
        }
        let file = w.finish().unwrap();
        let traks: Vec<&[u8]> =
            boxes(find(&file, &[b"moov"])).into_iter().filter(|b| &b.0 == b"trak").map(|b| b.1).collect();
        assert_eq!(traks.len(), 2);
        let audio = traks[1];
        assert_eq!(&find(audio, &[b"mdia", b"hdlr"])[8..12], b"soun");
        assert_eq!(be32(find(audio, &[b"mdia", b"mdhd"]), 12), 44100);
        let stsd = find(audio, &[b"mdia", b"minf", b"stbl", b"stsd"]);
        let mp4a = &stsd[8..];
        assert_eq!(&mp4a[4..8], b"mp4a");
        assert_eq!(u16::from_be_bytes([mp4a[24], mp4a[25]]), 2, "channels");
        assert_eq!(be32(mp4a, 32), 44100 << 16);
        let esds = boxes(&mp4a[36..]);
        assert_eq!(&esds[0].0, b"esds");
        assert!(
            esds[0].1.windows(4).any(|w| w == [5, 2, 0x12, 0x10]),
            "the AudioSpecificConfig is in a DecoderSpecificInfo"
        );
        let (runs, sizes, keys, offset) = tables(audio);
        assert_eq!((runs, sizes.clone(), keys), (vec![(3, 1024)], vec![10, 10, 10], None));
        let video_offset = tables(traks[0]).3;
        assert_eq!(offset, video_offset + 4, "the audio chunk follows the video chunk");
        assert_eq!(&file[offset as usize..], [[0u8; 10], [1; 10], [2; 10]].concat());
    }

    #[test]
    fn an_opus_track_has_a_dops_box() {
        let mut w = Mp4Writer::new(16, 16, 60, &AVCC).unwrap();
        w.set_audio(Audio::Opus { pre_skip: 312, input_rate: 32000 }, 48000, 1).unwrap();
        w.push_video(&[1], 1, true).unwrap();
        w.push_audio(&[7; 5], 960).unwrap();
        let file = w.finish().unwrap();
        let traks: Vec<&[u8]> =
            boxes(find(&file, &[b"moov"])).into_iter().filter(|b| &b.0 == b"trak").map(|b| b.1).collect();
        let stsd = find(traks[1], &[b"mdia", b"minf", b"stbl", b"stsd"]);
        assert_eq!(&stsd[12..16], b"Opus");
        let dops = boxes(&stsd[8 + 36..]);
        assert_eq!(&dops[0].0, b"dOps");
        assert_eq!(dops[0].1, [0, 1, 0x01, 0x38, 0, 0, 0x7D, 0, 0, 0, 0]);
    }

    #[test]
    fn bad_input_is_refused() {
        assert!(Mp4Writer::new(0, 16, 60, &AVCC).is_err());
        assert!(Mp4Writer::new(16, 16, 0, &AVCC).is_err());
        assert!(Mp4Writer::new(16, 16, 60, &[1, 2]).is_err(), "an avcC is at least 7 bytes");
        assert!(Mp4Writer::new(16, 16, 60, &[2, 0x42, 0xC0, 0x1E, 0xFF, 0xE1, 0, 0]).is_err(), "avcC version 1 only");
        let mut w = Mp4Writer::new(16, 16, 60, &AVCC).unwrap();
        assert!(w.push_video(&[], 1, true).is_err());
        assert!(w.push_audio(&[1], 1).is_err(), "no audio track was declared");
        assert!(Mp4Writer::new(16, 16, 60, &AVCC).unwrap().finish().is_err(), "no samples is not a video");
        let mut w = Mp4Writer::new(16, 16, 60, &AVCC).unwrap();
        assert!(w.set_audio(Audio::Aac { config: vec![] }, 44100, 2).is_err());
        assert!(w.set_audio(Audio::Aac { config: vec![0; 200] }, 44100, 2).is_err());
        assert!(w.set_audio(Audio::Aac { config: vec![0x12, 0x10] }, 0, 2).is_err());
    }

    /// Real encoder output through the muxer, judged by ffprobe and a full decode: skipped when ffmpeg is not installed.
    #[test]
    fn ffmpeg_accepts_a_remux_of_real_h264_and_aac() {
        use std::process::{Command, Stdio};
        let ffmpeg = |args: &[&str]| {
            Command::new("ffmpeg").args(["-v", "error", "-y"]).args(args).stderr(Stdio::piped()).output()
        };
        let dir = std::env::temp_dir().join(format!("gamemeta-mp4-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (src, out) = (dir.join("src.mp4"), dir.join("out.mp4"));
        let made = ffmpeg(&[
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=128x96:rate=60",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=44100",
            "-t",
            "1",
            "-c:v",
            "libx264",
            "-profile:v",
            "baseline",
            "-bf",
            "0",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-b:a",
            "64k",
            src.to_str().unwrap(),
        ]);
        let Some(made) = made.ok().filter(|m| m.status.success()) else {
            eprintln!("skipped: no ffmpeg with libx264 and aac");
            let _ = std::fs::remove_dir(&dir);
            return;
        };
        assert!(made.stderr.is_empty());
        let file = std::fs::read(&src).unwrap();
        let moov = find(&file, &[b"moov"]);
        let traks: Vec<&[u8]> = boxes(moov).into_iter().filter(|b| &b.0 == b"trak").map(|b| b.1).collect();
        // Samples of a track: sizes from stsz, bytes from the chunks (stsc, stco).
        let samples = |trak: &[u8]| -> Vec<Vec<u8>> {
            let stbl = find(trak, &[b"mdia", b"minf", b"stbl"]);
            let (stsz, stco, stsc) = (find(stbl, &[b"stsz"]), find(stbl, &[b"stco"]), find(stbl, &[b"stsc"]));
            let sizes: Vec<usize> = (0..be32(stsz, 8) as usize).map(|i| be32(stsz, 12 + 4 * i) as usize).collect();
            let per_chunk: Vec<(u32, u32)> =
                (0..be32(stsc, 4) as usize).map(|i| (be32(stsc, 8 + 12 * i), be32(stsc, 12 + 12 * i))).collect();
            let (mut out, mut next) = (vec![], 0);
            for c in 0..be32(stco, 4) {
                let n = per_chunk.iter().rev().find(|(first, _)| *first <= c + 1).unwrap().1 as usize;
                let mut at = be32(stco, 8 + 4 * c as usize) as usize;
                for _ in 0..n {
                    out.push(file[at..at + sizes[next]].to_vec());
                    at += sizes[next];
                    next += 1;
                }
            }
            out
        };
        let entry = |trak: &[u8]| find(trak, &[b"mdia", b"minf", b"stbl", b"stsd"])[8..].to_vec();
        let avcc = boxes(&entry(traks[0])[86..]).into_iter().find(|b| &b.0 == b"avcC").unwrap().1.to_vec();
        let mp4a = entry(traks[1]);
        let esds = boxes(&mp4a[36..]).into_iter().find(|b| &b.0 == b"esds").unwrap().1.to_vec();
        // ES descriptor (3) > decoder config (4) > decoder specific info (5); lengths may be 0x80-continued (ffmpeg does).
        let descriptor = |b: &[u8]| -> (u8, usize, usize) {
            let mut at = 1;
            let mut len = 0usize;
            loop {
                len = len << 7 | usize::from(b[at] & 0x7F);
                at += 1;
                if b[at - 1] & 0x80 == 0 {
                    return (b[0], at, len);
                }
            }
        };
        let (tag, head, _) = descriptor(&esds[4..]);
        assert_eq!(tag, 3);
        let es = &esds[4 + head + 3..];
        let (tag, head, _) = descriptor(es);
        assert_eq!(tag, 4);
        let specific = &es[head + 13..];
        let (tag, head, len) = descriptor(specific);
        assert_eq!(tag, 5);
        let asc = specific[head..head + len].to_vec();
        let mut w = Mp4Writer::new(128, 96, 60, &avcc).unwrap();
        w.set_audio(Audio::Aac { config: asc }, 44100, 1).unwrap();
        let video = samples(traks[0]);
        assert_eq!(video.len(), 60);
        for (i, v) in video.iter().enumerate() {
            w.push_video(v, 1, i == 0).unwrap();
        }
        for a in samples(traks[1]) {
            w.push_audio(&a, 1024).unwrap();
        }
        std::fs::write(&out, w.finish().unwrap()).unwrap();
        let probe = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=codec_name,width,height,nb_frames",
                "-of",
                "csv=p=0",
                out.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&probe.stdout).to_string();
        assert!(probe.status.success() && probe.stderr.is_empty(), "{text} {}", String::from_utf8_lossy(&probe.stderr));
        assert!(text.contains("h264,128,96,60"), "{text}");
        assert!(text.contains("aac"), "{text}");
        let decoded = ffmpeg(&["-i", out.to_str().unwrap(), "-f", "null", "-"]).unwrap();
        assert!(decoded.status.success() && decoded.stderr.is_empty(), "{}", String::from_utf8_lossy(&decoded.stderr));
        std::fs::remove_file(&src).unwrap();
        std::fs::remove_file(&out).unwrap();
        std::fs::remove_dir(&dir).unwrap();
    }
}
