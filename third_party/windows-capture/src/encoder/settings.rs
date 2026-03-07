use windows::core::HSTRING;
use windows::Media::MediaProperties::{
    AudioEncodingProperties, ContainerEncodingProperties, VideoEncodingProperties,
};

use super::VideoEncoderError;

/// The `VideoSettingsBuilder` struct is used to configure settings for the video encoder.
pub struct VideoSettingsBuilder {
    sub_type: VideoSettingsSubType,
    bitrate: u32,
    width: u32,
    height: u32,
    frame_rate: u32,
    pixel_aspect_ratio: (u32, u32),
    disabled: bool,
}

impl VideoSettingsBuilder {
    pub const fn new(width: u32, height: u32) -> Self {
        Self {
            bitrate: 15_000_000,
            frame_rate: 60,
            pixel_aspect_ratio: (1, 1),
            sub_type: VideoSettingsSubType::HEVC,
            width,
            height,
            disabled: false,
        }
    }

    pub const fn sub_type(mut self, sub_type: VideoSettingsSubType) -> Self {
        self.sub_type = sub_type;
        self
    }

    pub const fn bitrate(mut self, bitrate: u32) -> Self {
        self.bitrate = bitrate;
        self
    }

    pub const fn width(mut self, width: u32) -> Self {
        self.width = width;
        self
    }

    pub const fn height(mut self, height: u32) -> Self {
        self.height = height;
        self
    }

    pub const fn frame_rate(mut self, frame_rate: u32) -> Self {
        self.frame_rate = frame_rate;
        self
    }

    pub const fn pixel_aspect_ratio(mut self, pixel_aspect_ratio: (u32, u32)) -> Self {
        self.pixel_aspect_ratio = pixel_aspect_ratio;
        self
    }

    pub const fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub(super) const fn target_frame_rate(&self) -> u32 {
        self.frame_rate
    }

    pub(super) fn build(self) -> Result<(VideoEncodingProperties, bool), VideoEncoderError> {
        let properties = VideoEncodingProperties::new()?;

        properties.SetSubtype(&self.sub_type.to_hstring())?;
        properties.SetBitrate(self.bitrate)?;
        properties.SetWidth(self.width)?;
        properties.SetHeight(self.height)?;
        properties.FrameRate()?.SetNumerator(self.frame_rate)?;
        properties.FrameRate()?.SetDenominator(1)?;
        properties
            .PixelAspectRatio()?
            .SetNumerator(self.pixel_aspect_ratio.0)?;
        properties
            .PixelAspectRatio()?
            .SetDenominator(self.pixel_aspect_ratio.1)?;

        Ok((properties, self.disabled))
    }
}

/// The `AudioSettingsBuilder` is used to configure settings for the audio encoder.
pub struct AudioSettingsBuilder {
    bitrate: u32,
    channel_count: u32,
    sample_rate: u32,
    bit_per_sample: u32,
    sub_type: AudioSettingsSubType,
    disabled: bool,
}

impl AudioSettingsBuilder {
    pub const fn new() -> Self {
        Self {
            bitrate: 192_000,
            channel_count: 2,
            sample_rate: 48_000,
            bit_per_sample: 16,
            sub_type: AudioSettingsSubType::AAC,
            disabled: false,
        }
    }

    pub const fn bitrate(mut self, bitrate: u32) -> Self {
        self.bitrate = bitrate;
        self
    }

    pub const fn channel_count(mut self, channel_count: u32) -> Self {
        self.channel_count = channel_count;
        self
    }

    pub const fn sample_rate(mut self, sample_rate: u32) -> Self {
        self.sample_rate = sample_rate;
        self
    }

    pub const fn bit_per_sample(mut self, bit_per_sample: u32) -> Self {
        self.bit_per_sample = bit_per_sample;
        self
    }

    pub const fn sub_type(mut self, sub_type: AudioSettingsSubType) -> Self {
        self.sub_type = sub_type;
        self
    }

    pub const fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub(super) fn build(self) -> Result<(AudioEncodingProperties, bool), VideoEncoderError> {
        let properties = AudioEncodingProperties::new()?;
        properties.SetBitrate(self.bitrate)?;
        properties.SetChannelCount(self.channel_count)?;
        properties.SetSampleRate(self.sample_rate)?;
        properties.SetBitsPerSample(self.bit_per_sample)?;
        properties.SetSubtype(&self.sub_type.to_hstring())?;

        Ok((properties, self.disabled))
    }
}

impl Default for AudioSettingsBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// The `ContainerSettingsBuilder` is used to configure settings for the container.
pub struct ContainerSettingsBuilder {
    sub_type: ContainerSettingsSubType,
}

impl ContainerSettingsBuilder {
    pub const fn new() -> Self {
        Self {
            sub_type: ContainerSettingsSubType::MPEG4,
        }
    }

    pub const fn sub_type(mut self, sub_type: ContainerSettingsSubType) -> Self {
        self.sub_type = sub_type;
        self
    }

    pub(super) fn build(self) -> Result<ContainerEncodingProperties, VideoEncoderError> {
        let properties = ContainerEncodingProperties::new()?;
        properties.SetSubtype(&self.sub_type.to_hstring())?;
        Ok(properties)
    }
}

impl Default for ContainerSettingsBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// The `VideoSettingsSubType` enum represents the subtypes for the video encoder.
#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum VideoSettingsSubType {
    ARGB32,
    BGRA8,
    D16,
    H263,
    H264,
    H264ES,
    HEVC,
    HEVCES,
    IYUV,
    L8,
    L16,
    MJPG,
    NV12,
    MPEG1,
    MPEG2,
    RGB24,
    RGB32,
    WMV3,
    WVC1,
    VP9,
    YUY2,
    YV12,
}

impl VideoSettingsSubType {
    pub fn to_hstring(&self) -> HSTRING {
        let subtype = match self {
            Self::ARGB32 => "ARGB32",
            Self::BGRA8 => "BGRA8",
            Self::D16 => "D16",
            Self::H263 => "H263",
            Self::H264 => "H264",
            Self::H264ES => "H264ES",
            Self::HEVC => "HEVC",
            Self::HEVCES => "HEVCES",
            Self::IYUV => "IYUV",
            Self::L8 => "L8",
            Self::L16 => "L16",
            Self::MJPG => "MJPG",
            Self::NV12 => "NV12",
            Self::MPEG1 => "MPEG1",
            Self::MPEG2 => "MPEG2",
            Self::RGB24 => "RGB24",
            Self::RGB32 => "RGB32",
            Self::WMV3 => "WMV3",
            Self::WVC1 => "WVC1",
            Self::VP9 => "VP9",
            Self::YUY2 => "YUY2",
            Self::YV12 => "YV12",
        };

        HSTRING::from(subtype)
    }
}

/// The `AudioSettingsSubType` enum represents the subtypes for the audio encoder.
#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum AudioSettingsSubType {
    AAC,
    AC3,
    AACADTS,
    AACHDCP,
    AC3SPDIF,
    AC3HDCP,
    ADTS,
    ALAC,
    AMRNB,
    AWRWB,
    DTS,
    EAC3,
    FLAC,
    Float,
    MP3,
    MPEG,
    OPUS,
    PCM,
    WMA8,
    WMA9,
    Vorbis,
}

impl AudioSettingsSubType {
    pub fn to_hstring(&self) -> HSTRING {
        let subtype = match self {
            Self::AAC => "AAC",
            Self::AC3 => "AC3",
            Self::AACADTS => "AACADTS",
            Self::AACHDCP => "AACHDCP",
            Self::AC3SPDIF => "AC3SPDIF",
            Self::AC3HDCP => "AC3HDCP",
            Self::ADTS => "ADTS",
            Self::ALAC => "ALAC",
            Self::AMRNB => "AMRNB",
            Self::AWRWB => "AWRWB",
            Self::DTS => "DTS",
            Self::EAC3 => "EAC3",
            Self::FLAC => "FLAC",
            Self::Float => "Float",
            Self::MP3 => "MP3",
            Self::MPEG => "MPEG",
            Self::OPUS => "OPUS",
            Self::PCM => "PCM",
            Self::WMA8 => "WMA8",
            Self::WMA9 => "WMA9",
            Self::Vorbis => "Vorbis",
        };

        HSTRING::from(subtype)
    }
}

/// The `ContainerSettingsSubType` enum represents the subtypes for the container.
#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum ContainerSettingsSubType {
    ASF,
    MP3,
    MPEG4,
    AVI,
    MPEG2,
    WAVE,
    AACADTS,
    ADTS,
    GP3,
    AMR,
    FLAC,
}

impl ContainerSettingsSubType {
    pub fn to_hstring(&self) -> HSTRING {
        match self {
            Self::ASF => HSTRING::from("ASF"),
            Self::MP3 => HSTRING::from("MP3"),
            Self::MPEG4 => HSTRING::from("MPEG4"),
            Self::AVI => HSTRING::from("AVI"),
            Self::MPEG2 => HSTRING::from("MPEG2"),
            Self::WAVE => HSTRING::from("WAVE"),
            Self::AACADTS => HSTRING::from("AACADTS"),
            Self::ADTS => HSTRING::from("ADTS"),
            Self::GP3 => HSTRING::from("3GP"),
            Self::AMR => HSTRING::from("AMR"),
            Self::FLAC => HSTRING::from("FLAC"),
        }
    }
}
