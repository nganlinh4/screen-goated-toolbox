#[repr(C)]
pub enum NVencParamsFrameFieldMode {
    Frame = 0x01,
    Field = 0x02,
    MBAFF = 0x03,
}

#[repr(C)]
pub enum NVencParamsRcMode {
    ConstQP = 0x00,
    VBR = 0x01,
    CBR = 0x02,
}

#[repr(C)]
pub enum NVencMultiPass {
    Disabled = 0x00,
    TwoPassQuarterResolution = 0x01,
    TwoPassFullResolution = 0x02,
}

#[repr(C)]
pub enum NVencStateRestoreType {
    Full = 0x01,
    RateControl = 0x02,
    Encode = 0x03,
}

#[repr(C)]
pub enum NVencOutputStatsLevel {
    None = 0,
    BlockLevel = 1,
    RowLevel = 2,
}

#[repr(C)]
pub enum NVencEmphasisMapLevel {
    Level0 = 0x0,
    Level1 = 0x1,
    Level2 = 0x2,
    Level3 = 0x3,
    Level4 = 0x4,
    Level5 = 0x5,
}

#[repr(C)]
pub enum NVencQPMapMode {
    Disabled = 0x0,
    Emphasis = 0x1,
    Delta = 0x2,
    /// Currently unsupported as of NVENC 13.0
    Map = 0x3,
}

#[repr(C)]
pub enum NVencPicStruct {
    Frame = 0x01,
    FieldTopBottom = 0x02,
    FieldBottomTop = 0x03,
}

#[repr(C)]
pub enum NVencDisplayPicStruct {
    Frame = 0x0,
    FieldTopBottom = 0x1,
    FieldBottomTop = 0x2,
    Doubling = 0x3,
    Tripling = 0x4,
}

#[repr(C)]
pub enum NVencPicType {
    P = 0x00,
    B = 0x01,
    IDR = 0x03,
    BI = 0x04,
    Skipped = 0x05,
    IntraRefresh = 0x06,
    NonRefP = 0x07,
    Switch = 0x08,
    UNKNOWN = 0xFF,
}

#[repr(C)]
pub enum NVencMVPrecision {
    Default = 0x00,
    FullPel = 0x01,
    HalfPel = 0x02,
    QuarterPel = 0x03,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub enum NVencBufferFormat {
    Undefined = 0x00000000,
    NV12 = 0x00000001,
    YV12 = 0x00000010,
    IYUV = 0x00000100,
    YUV444 = 0x00001000,
    YUV420_10Bit = 0x00010000,
    YUV444_10Bit = 0x00100000,
    ARGB = 0x01000000,
    ARGB10 = 0x02000000,
    AYUV = 0x04000000,
    ABGR = 0x10000000,
    ABGR10 = 0x20000000,
    U8 = 0x40000000,
    NV16 = 0x40000001,
    P210 = 0x40000002,
}

#[repr(C)]
pub enum NvencLevel {
    AutoSelect = 0,
}

// TODO: Translate level macro
#[repr(C)]
pub enum H264Level {
    H264_1 = 10,
    H264_1b = 9,
    H264_11 = 11,
    H264_12 = 12,
    H264_13 = 13,
    H264_2 = 20,
    H264_21 = 21,
    H264_22 = 22,
    H264_3 = 30,
    H264_31 = 31,
    H264_32 = 32,
    H264_4 = 40,
    H264_41 = 41,
    H264_42 = 42,
    H264_5 = 50,
    H264_51 = 51,
    H264_52 = 52,
    H264_60 = 60,
    H264_61 = 61,
    H264_62 = 62,
}

#[repr(C)]
pub enum HEVCLevel {
    HEVC1 = 30,
    HEVC2 = 60,
    HEVC21 = 63,
    HEVC3 = 90,
    HEVC31 = 93,
    HEVC4 = 120,
    HEVC41 = 123,
    HEVC5 = 150,
    HEVC51 = 153,
    HEVC52 = 156,
    HEVC6 = 180,
    HEVC61 = 183,
    HEVC62 = 186,
    HEVCMain = 0,
    HEVCHigh = 1,
}

#[repr(C)]
pub enum AV1Level {
    // Also AV1_0
    AV1_2 = 0,
    // Also AV1_1
    AV1_21 = 1,
    AV1_22 = 2,
    AV1_23 = 3,
    AV1_3 = 4,
    AV1_31 = 5,
    AV1_32 = 6,
    AV1_33 = 7,
    AV1_4 = 8,
    AV1_41 = 9,
    AV1_42 = 10,
    AV1_43 = 11,
    AV1_5 = 12,
    AV1_51 = 13,
    AV1_52 = 14,
    AV1_53 = 15,
    AV1_6 = 16,
    AV1_61 = 17,
    AV1_62 = 18,
    AV1_63 = 19,
    AV1_7 = 20,
    AV1_71 = 21,
    AV1_72 = 22,
    AV1_73 = 23,
    AV1AutoSelect = 24,
}

#[repr(C)]
pub enum NVencPicFlags {
    ForceIntra = 0x1,
    ForceIDR = 0x2,
    OutputSpspps = 0x4,
    Eos = 0x8,
    DisableEncStateAdvance = 0x10,
    OuputReconFrame = 0x20,
}

#[repr(C)]
pub enum NVencMemoryHeap {
    AutoSelect = 0,
    VID = 1,
    SystemCached = 2,
    SystemUncached = 3,
}

#[repr(C)]
pub enum NVencBFrameRefMode {
    Disabled = 0x0,
    Each = 0x1,
    Middle = 0x2,
}

#[repr(C)]
pub enum NVencH264EntropyCodingMode {
    AutoSelect = 0x0,
    Cabac = 0x1,
    Cavlc = 0x2,
}

#[repr(C)]
pub enum NVencH264BDirectMode {
    Autoselect = 0x0,
    Disable = 0x1,
    Temporal = 0x2,
    Spatial = 0x3,
}

#[repr(C)]
pub enum NVencH264FMOMode {
    AutoSelect = 0x0,
    Enable = 0x1,
    Disable = 0x2,
}

#[repr(C)]
pub enum NVencH264AdaptiveTransformMode {
    AutoSelect = 0x0,
    Disable = 0x1,
    Enable = 0x2,
}

#[repr(C)]
pub enum NVencStereoPackingMode {
    None = 0x0,
    Checkerboard = 0x1,
    ColInterleave = 0x2,
    RowInterleave = 0x3,
    SideBySide = 0x4,
    TopBottm = 0x5,
    FrameSeq = 0x6,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub enum NVencInputResourceType {
    DirectX = 0x0,
    CudaDeivcePtr = 0x1,
    CudaArray = 0x2,
    OpenGLTex = 0x3,
}

#[repr(C)]
pub enum NVencBufferUsage {
    Image = 0x0,
    MotionVector = 0x1,
    BitStream = 0x2,
    OutputRecon = 0x4,
}

#[repr(C)]
#[derive(Clone)]
pub enum NVencDeviceType {
    DirectX = 0x0,
    Cuda = 0x1,
    OpenGL = 0x2,
}

#[repr(C)]
pub enum NVencNumRefFrames {
    AutoSelect = 0x0,
    Frames1 = 0x1,
    Frames2 = 0x2,
    Frames3 = 0x3,
    Frames4 = 0x4,
    Frames5 = 0x5,
    Frames6 = 0x6,
    Frames7 = 0x7,
}

#[repr(C)]
pub enum NVencTemporalFilterLevel {
    Level0 = 0,
    Level4 = 4,
}

#[repr(C)]
pub enum NVencCaps {
    NumMaxBFrames,
    SupportedRateControlModes,
    FieldEncoding,
    MonoChrome,
    FMO,
    QPELMV,
    BDirectMode,
    CABAC,
    AdaptiveTransform,
    StereoMVC,
    MaxTemporalLayers,
    HierarchicalBFrames,
    LevelMax,
    LevelMin,
    SeperateColorPlane,
    WidthMax,
    HeightMax,
    TemporalSVC,
    DynResChnage,
    DynBitrateChange,
    DynForceConstQP,
    DynRcModeChange,
    SubframeReadBack,
    ConstrainedEncoding,
    IntraRefresh,
    CustomVBVBufSize,
    DynamicSliceMode,
    Invalidation,
    PreProcSupport,
    AsyncEncodeSupport,
    MbNumMax,
    MbPerSecMax,
    YUV444Encode,
    LoselessEncode,
    SupportSao,
    MeOnlyMode,
    LookAhead,
    TemporalAQ,
    MaxLtrFrames,
    SupportWeightedPrediction,
    DynamicQueryEncoderCapacity,
    BFrameRefMode,
    EmphasisLevelMap,
    WidthMin,
    HeightMin,
    MultipleRefFrames,
    AlphaLayerEncoding,
    NumEncoderEngines,
    SingleSliceIntraRefresh,
    DisableEncStateAdvance,
    OutputReconSurface,
    OutputBlockStats,
    SupportTemporalFilter,
    LookAheadLevel,
    UndirectionalB,
    MVHEVCEncode,
    SupportYUV422Encode,
    ExposedCount,
}

#[repr(C)]
pub enum NVencHEVCCuSize {
    AutoSelect = 0,
    Size8x8 = 1,
    Size16x16 = 2,
    Size32x32 = 3,
    Size64x64 = 4,
}

#[repr(C)]
pub enum NVencAV1PartSize {
    AutoSelect = 0,
    Size4x4 = 1,
    Size8x8 = 2,
    Size16x16 = 3,
    Size32x32 = 4,
    Size64x64 = 5,
}

#[repr(C)]
pub enum NVencVUIVideoFormat {
    Component = 0,
    Pal = 1,
    NTSC = 2,
    SeCam = 3,
    Mac = 4,
    Unspecified = 5,
}

#[repr(C)]
pub enum NVencVUIColorPrimaries {
    Undefined = 0,
    BT709 = 1,
    Unspecified = 2,
    Reserved = 3,
    BT470M = 4,
    BT470BG = 5,
    SMPTE170M = 6,
    SMPTE240M = 7,
    FIlm = 8,
    BT2020 = 9,
    SMPTE428 = 10,
    SMPTE431 = 11,
    SMPTE432 = 12,
    JEDECP22 = 22,
}

#[repr(C)]
pub enum NVencVUITransCharacteristics {
    Undefined = 0,
    BT709 = 1,
    Unspecified = 2,
    Reserved = 3,
    BT470M = 4,
    BT470BG = 5,
    SMPTE170M = 6,
    SMPTE240M = 7,
    Linear = 8,
    Log = 9,
    LogSqrt = 10,
    IEC61966_2_4 = 11,
    SRGB = 13,
    BT2020_10 = 14,
    BT2020_12 = 15,
    SMPTE2084 = 16,
    SMPTE428 = 17,
    AribStdB67 = 18,
}

#[repr(C)]
pub enum NVencVUIMatrixCoeffs {
    RGB = 0,
    BT709 = 1,
    Unspecified = 2,
    Reserved = 3,
    FCC = 4,
    BT470BG = 5,
    SMPTE170M,
    SMPTE240M,
    YCGCO = 8,
    BT2020NCL = 9,
    BT2020CL = 10,
    SMPTE2085 = 11,
}

#[repr(C)]
pub enum NVencLookAheadLevel {
    Level0 = 0,
    Level1 = 1,
    Level2 = 2,
    Level3 = 3,
    AutoSelect = 15,
}

#[repr(C)]
pub enum NVencBitDepth {
    Invalid = 0,
    Depth8 = 8,
    Depth10 = 10,
}

#[derive(Debug)]
#[repr(C)]
pub enum NVencTuningInfo {
    Undefined = 0,
    HighQuality = 1,
    LowLatency = 2,
    UltraLowLatency = 3,
    Lossless = 4,
    UltraHighQuality = 5,
    Count,
}

#[repr(C)]
pub enum NVencSplitEncodeMode {
    Audo = 0,
    AutoForced = 1,
    TwoForced = 2,
    ThreeForced = 3,
    FourForced = 4,
    Disable = 15,
}
