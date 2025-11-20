use std::fs::File;
use std::io::{Write, Result};

fn main() -> Result<()> {
    // Canvas dimensions
    let width = 32;
    let height = 32;

    // ==========================================
    // 0. CONFIGURATION (Added : f32 to fix the error)
    // ==========================================
    let scale: f32 = 0.75; 
    let border_radius: f32 = 1.0; 
    let blur_edge: f32 = 0.8; 
    
    // Original Hotspot
    let orig_hotspot_x: f32 = 1.0;
    let orig_hotspot_y: f32 = 27.0;

    // Calculate New Hotspot (centered scaling)
    let center: f32 = 15.5;
    let new_hotspot_x = (center + (orig_hotspot_x - center) * scale).round() as u16;
    let new_hotspot_y = (center + (orig_hotspot_y - center) * scale).round() as u16;
    
    println!("Hotspot updated: ({}, {}) -> ({}, {})", 
             orig_hotspot_x, orig_hotspot_y, new_hotspot_x, new_hotspot_y);

    // Source Pixels (32x32 ARGB)
    let raw_pixels: [u32; 1024] = [
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x0F8E8E8E, 0xB46C6866, 0xFF523222, 0xFF4D3224, 0xAD706C6A, 0x0B8E8E8E,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x40787776, 0xFF533527, 0xFF955F42, 0xFF6D3D24, 0xFF423028, 0x1E858585,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x36898989, 0xE355443C, 0xFF88543A, 0xFF713E27, 0xFF4C2615, 0xFF443631, 0x1E878787,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x018F8E8F, 0xF5625956, 0xFF6F422D, 0xFF804B30, 0xFF4E2717, 0xF63A2620, 0x59787878, 0x028F8F8F,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x6D797977, 0xFF4C3326, 0xFF864F34, 0xFF5B2F1B, 0xFF382219, 0x725E5C5C, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x1C898988,
        0xCB53453E, 0xFF8A5236, 0xFF6E3B23, 0xFF412418, 0x85636160, 0x238F8F8E, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x646A6462,
        0xFF6A3F2A, 0xFF774127, 0xFF462518, 0xD8504B4A, 0x0A8D8D8D, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x088B8A8A, 0xD1634538,
        0xFF834C31, 0xFF552B18, 0xE44A3E39, 0x29868585, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x038E8E8E, 0xAF524843, 0xFF7A472E,
        0xFF653620, 0xFF33231D, 0x94767675, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x90767574, 0xFF633A28, 0xFF774128,
        0xFF432214, 0x89565453, 0x098E8E8E, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x27848484, 0xDF463730, 0xFF894F32, 0xFF522917,
        0xFC32241D, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x81635C59, 0xFF75442D, 0xFF60321D, 0xFF371F15,
        0x68676664, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x1F888887,
        0x2A848483, 0x1E8D8C8C, 0x00000000, 0x5D7E7E7E, 0xF85A3B2D, 0xFF7E462C, 0xFF462214, 0xD2493E39,
        0x158A8A8A, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0xBB6F6B67,
        0xFF514635, 0xDF6A5D50, 0x94686867, 0xC561544F, 0xFF7B482F, 0xFF5E311D, 0xFF2A150D, 0x70706D6C,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0xBB7C746A,
        0xFF91602E, 0xFF9E6E38, 0xFF48341E, 0xFF3B2117, 0xFF5C301D, 0xFF412013, 0xD754514E, 0x1F8D8D8C,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0xBB736859,
        0xFFB68445, 0xFFBF8945, 0xFFB38244, 0xFF654424, 0xFF4A2D19, 0xFB2A1912, 0x3B848483, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x30828181, 0xDE8E7248,
        0xFFDEAC61, 0xFFD5A157, 0xFFBF8C4A, 0xFFBC8747, 0xFFAB773A, 0xFF7A5730, 0xC964564A, 0x19838382,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0xE83C302B, 0xFF674226,
        0xFFA0713B, 0xFFC79756, 0xFFBE8947, 0xFFBB813E, 0xFF9F672C, 0xFF92612A, 0xFF472D15, 0xBC5A5754,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x1E8C8C8C, 0xB6757271, 0xFB3E261C, 0xFF3A2015,
        0xFF422314, 0xFF644225, 0xFFA07038, 0xFF9B672F, 0xFF8F5B25, 0xFE7A4E22, 0xE95E5345, 0x58797773,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x74696969, 0xFF362015, 0xFF3F2517, 0xFF26150E,
        0xFF4E2C1C, 0xFF472718, 0xFF432617, 0xFF684120, 0xFF5A3A1C, 0xB058534D, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x0E8E8E8E, 0x3F7F7F7E, 0xDE685A48, 0xF88F6D41, 0xFFA47A45, 0xFF5A3D23, 0xFF492A19,
        0xFF392014, 0xFF341C11, 0xFF331B10, 0xFF27150D, 0xCA38302D, 0x0C8C8C8B, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x038F8F8F, 0xA47A7874, 0xFF7D6745, 0xFFB1874E, 0xFFB4864A, 0xFFC39250, 0xFFBD8D4F, 0xFF9A6D38,
        0xFF492A16, 0xFF371D10, 0xFF26140B, 0xFF1A0D09, 0x596D6B6A, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x088C8C8C,
        0xAF6F6B62, 0xFF957444, 0xFFB2864C, 0xFFC89959, 0xFFBE8E51, 0xFFA87941, 0xFFD7A65F, 0xFFBB8A4B,
        0xFFA57036, 0xFF6E441E, 0xFF4B2C14, 0xFF372C27, 0x21858585, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x3480807F, 0x9F6D6457,
        0xFF9C743F, 0xFFBF8F52, 0xFFC89451, 0xFFC08A4A, 0xFFC29253, 0xFFD4A461, 0xFFCF9D58, 0xFFB07B3E,
        0xFFB07C3E, 0xFF935D28, 0xFF6B3D17, 0xA14A4A4A, 0x068F8F8E, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x018F8F8F, 0x00000000, 0x8D717071, 0x975D5956, 0x9760584D, 0xF7715A3F, 0xFFB88849,
        0xFFD6A25D, 0xFFD49D57, 0xFFCD944E, 0xFFD59F59, 0xFFD6A45F, 0xFFE3B671, 0xFFC99654, 0xFFC68D48,
        0xFFBC8445, 0xFF895422, 0xFF72441B, 0x8B4D4B4A, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0xAF808080, 0x60606060, 0x985B5B5A, 0xFF363027, 0xFF4A371F, 0xFFBB8B4C, 0xFFB17B41,
        0xFFD19B55, 0xFFC9924E, 0xFFD5A058, 0xFFD59F59, 0xFFD5A664, 0xFFDDAE68, 0xFFB98242, 0xFFD09B55,
        0xFF9D642A, 0xFF905A22, 0xF7704A23, 0x57787875, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x0F8E8E8E, 0x72727271, 0xFF443E39, 0xFF42331F, 0xFF7E5A30, 0xFFBE8745, 0xFFC28C48, 0xFFB98142,
        0xFFB67F3F, 0xFFD8A762, 0xFFD5A059, 0xFFCD9753, 0xFFC99654, 0xFFD09C55, 0xFFC4914E, 0xFFBE8644,
        0xFFAC7535, 0xFF865220, 0xE9573A20, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x2D8C8C8C, 0xE23E3A37, 0xFF55371C, 0xFF9D6E39, 0xFFBD8848, 0xFFBC8748, 0xFFA6713A, 0xFFB67F42,
        0xFFD49F58, 0xFFD29C57, 0xFFC28B4A, 0xFFB87F41, 0xFFC28845, 0xFFB47B3C, 0xFFC58E4D, 0xFFAF7737,
        0xFF905824, 0xFF7A481B, 0xE9564A3C, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x966A6663, 0xFF704723, 0xFFA76E35, 0xFFB67D3E, 0xFFA9733A, 0xFFC08D4F, 0xFFC38945,
        0xFFC5904E, 0xFFB67D3E, 0xFF9F652D, 0xFFB67B3D, 0xFFB17A3E, 0xFFCC9854, 0xFFB7803E, 0xFF925C28,
        0xFF824E1E, 0xFF68401E, 0x527A756F, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x0B8A8A8A, 0xA15A524B, 0xFD633E1E, 0xFF976330, 0xFFAD7B43, 0xFFB37F43, 0xFFBB8444,
        0xFFA87137, 0xFFB1783A, 0xFF9F662E, 0xFFAD763C, 0xFFB17B3F, 0xFFA16C34, 0xFF8F5A26, 0xFF7D491C,
        0xFF74431A, 0xC7564737, 0x14898989, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0xA3645F5C, 0xFF4C3320, 0xFF6D3E19, 0xFF8F5928, 0xFF97602A,
        0xFF935E28, 0xFF99632D, 0xFFAD783B, 0xFF9C6730, 0xFF9A642E, 0xFF855122, 0xFF7F4B1F, 0xFF653E1A,
        0xFF4E4132, 0x60868583, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x0B8E8D8D, 0x337B7A7A, 0xDA473F3A, 0xFF5A3A26, 0xFF522D13,
        0xFF77461D, 0xFF834F21, 0xFF8E5825, 0xFF845223, 0xFF6B401B, 0xFF6B411C, 0xFF62462E, 0xB267625D,
        0x1E878786, 0x028F8E8E, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000
    ];

    // ==========================================
    // 1. GENERATE SCALED IMAGE
    // ==========================================
    let mut scaled_buffer = vec![0u32; 1024];

    // Bilinear interpolation helper
    let get_color = |x: f32, y: f32| -> (f32, f32, f32, f32) {
        if x < 0.0 || y < 0.0 || x >= (width - 1) as f32 || y >= (height - 1) as f32 {
            return (0.0, 0.0, 0.0, 0.0);
        }
        let x_l = x.floor() as usize;
        let y_l = y.floor() as usize;
        let x_h = x_l + 1;
        let y_h = y_l + 1;
        let dx = x - x_l as f32;
        let dy = y - y_l as f32;

        let get_px = |ix, iy| {
            let p = raw_pixels[iy * 32 + ix];
            let a = (p >> 24) as u8 as f32;
            let r = (p >> 16) as u8 as f32;
            let g = (p >> 8) as u8 as f32;
            let b = (p & 0xFF) as u8 as f32;
            (a, r, g, b)
        };

        let c00 = get_px(x_l, y_l);
        let c10 = get_px(x_h, y_l);
        let c01 = get_px(x_l, y_h);
        let c11 = get_px(x_h, y_h);

        let blend = |v00, v10, v01, v11| {
            let top = v00 * (1.0 - dx) + v10 * dx;
            let bot = v01 * (1.0 - dx) + v11 * dx;
            top * (1.0 - dy) + bot * dy
        };

        (
            blend(c00.0, c10.0, c01.0, c11.0),
            blend(c00.1, c10.1, c01.1, c11.1),
            blend(c00.2, c10.2, c01.2, c11.2),
            blend(c00.3, c10.3, c01.3, c11.3),
        )
    };

    // Apply scaling logic
    for y in 0..height {
        for x in 0..width {
            // Map dst(x,y) -> src(x,y)
            // (x - cx) / scale + cx
            let src_x = (x as f32 - center) / scale + center;
            let src_y = (y as f32 - center) / scale + center;
            
            let (a, r, g, b) = get_color(src_x, src_y);
            
            let pix = ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
            scaled_buffer[y * 32 + x] = pix;
        }
    }

    // ==========================================
    // 2. COMPOSITE WITH AA BORDER
    // ==========================================
    let mut final_buffer = vec![0u32; 1024];

    for y in 0..height {
        for x in 0..width {
            let idx = y * 32 + x;
            let fg = scaled_buffer[idx];
            let fg_a = ((fg >> 24) & 0xFF) as f32 / 255.0;
            let fg_r = ((fg >> 16) & 0xFF) as f32 / 255.0;
            let fg_g = ((fg >> 8) & 0xFF) as f32 / 255.0;
            let fg_b = (fg & 0xFF) as f32 / 255.0;

            // Calculate distance to nearest opaque pixel in scaled_buffer
            // This creates a smooth "SDF" (Signed Distance Field) outline
            let mut min_dist = 100.0f32;
            
            // Optimization: Only scan relevant area
            let scan_radius = (border_radius + 2.0) as i32;
            for dy in -scan_radius..=scan_radius {
                for dx in -scan_radius..=scan_radius {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx >= 0 && nx < 32 && ny >= 0 && ny < 32 {
                        let n_idx = (ny * 32 + nx) as usize;
                        // If pixel is opaque enough to be considered "body"
                        if (scaled_buffer[n_idx] >> 24) > 128 {
                            let dist = ((dx*dx + dy*dy) as f32).sqrt();
                            if dist < min_dist {
                                min_dist = dist;
                            }
                        }
                    }
                }
            }

            // Border Logic
            // If min_dist < border_radius, we are inside the border.
            // Use 'blur_edge' to smooth the transition.
            let mut border_alpha = 0.0;
            if min_dist < border_radius + blur_edge {
                let val = (border_radius + blur_edge - min_dist) / blur_edge;
                border_alpha = val.clamp(0.0, 1.0);
            }

            // Composition: (FG over Border) over Background(Transparent)
            // Border Color: White
            let b_r = 1.0; let b_g = 1.0; let b_b = 1.0;
            
            // Result Alpha
            // A_out = A_fg + A_border * (1 - A_fg)
            let out_a = fg_a + border_alpha * (1.0 - fg_a);
            
            if out_a > 0.0 {
                // Result Color (Premultiplied logic simplified)
                // C_out = (C_fg * A_fg + C_border * A_border * (1 - A_fg)) / A_out
                let out_r = (fg_r * fg_a + b_r * border_alpha * (1.0 - fg_a)) / out_a;
                let out_g = (fg_g * fg_a + b_g * border_alpha * (1.0 - fg_a)) / out_a;
                let out_b = (fg_b * fg_a + b_b * border_alpha * (1.0 - fg_a)) / out_a;

                final_buffer[idx] = 
                    ((out_a * 255.0) as u32) << 24 |
                    ((out_r * 255.0) as u32) << 16 |
                    ((out_g * 255.0) as u32) << 8 |
                    ((out_b * 255.0) as u32);
            }
        }
    }

    // ==========================================
    // 3. WRITE CURSOR FILE
    // ==========================================
    let mut cursor_data = Vec::new();

    // ICONDIR
    cursor_data.extend_from_slice(&[0, 0, 2, 0, 1, 0]); 

    // ICONDIRENTRY
    cursor_data.push(width as u8);
    cursor_data.push(height as u8);
    cursor_data.push(0);
    cursor_data.push(0);
    cursor_data.extend_from_slice(&new_hotspot_x.to_le_bytes());
    cursor_data.extend_from_slice(&new_hotspot_y.to_le_bytes());
    
    let size_offset_idx = cursor_data.len();
    cursor_data.extend_from_slice(&[0, 0, 0, 0]); // Size placeholder
    cursor_data.extend_from_slice(&[22, 0, 0, 0]); // Offset

    let bmp_start = cursor_data.len();

    // BITMAPINFOHEADER
    cursor_data.extend_from_slice(&40u32.to_le_bytes());
    cursor_data.extend_from_slice(&(width as i32).to_le_bytes());
    cursor_data.extend_from_slice(&((height * 2) as i32).to_le_bytes());
    cursor_data.extend_from_slice(&1u16.to_le_bytes());
    cursor_data.extend_from_slice(&32u16.to_le_bytes());
    cursor_data.extend_from_slice(&[0; 24]); // Compression, Size, etc. (0 is fine for uncompressed)

    // Pixel Data (Bottom-Up)
    for y in (0..height).rev() {
        for x in 0..width {
            let idx = y * width + x;
            let pixel = final_buffer[idx];
            let a = (pixel >> 24) as u8;
            let r = (pixel >> 16) as u8;
            let g = (pixel >> 8) as u8;
            let b = pixel as u8;
            cursor_data.extend_from_slice(&[b, g, r, a]);
        }
    }

    // AND Mask (Transparency Mask for compatibility)
    for y in (0..height).rev() {
        let mut byte = 0u8;
        for x in 0..width {
            let idx = y * width + x;
            // If pixel is mostly transparent, set mask bit to 1
            if (final_buffer[idx] >> 24) < 10 { 
                byte |= 1 << (7 - (x % 8)); 
            }
            if (x + 1) % 8 == 0 {
                cursor_data.push(byte);
                byte = 0;
            }
        }
        if width % 8 != 0 { cursor_data.push(byte); }
    }

    // Patch Size
    let size = (cursor_data.len() - bmp_start) as u32;
    let sb = size.to_le_bytes();
    cursor_data[size_offset_idx] = sb[0];
    cursor_data[size_offset_idx+1] = sb[1];
    cursor_data[size_offset_idx+2] = sb[2];
    cursor_data[size_offset_idx+3] = sb[3];

    let mut file = File::create("broom.cur")?;
    file.write_all(&cursor_data)?;

    println!("✅ Created 'broom.cur' with scaling and AA border!");
    Ok(())
}