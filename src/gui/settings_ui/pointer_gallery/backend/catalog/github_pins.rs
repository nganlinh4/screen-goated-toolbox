//! Commit-pinned GitHub cursor sources with the exact size and SHA-256 of
//! every file, in `REQUIRED_FILES` order. Changing a source means re-hashing
//! the files at a new immutable commit, never pointing at a branch.

use super::GithubSource;

#[rustfmt::skip]
pub(super) static MATERIAL_DEFAULT: GithubSource = GithubSource {
    raw_root: "https://raw.githubusercontent.com/SullensCR/Windows-Material-Design-Cursor-V2-Dark-Hdpi-by-jepriCreations/7c643edf4a39f94c056335fb0233a041ca7aa926",
    dirs: &["cursor/default"],
    files: [
        (0, 68966, "9f4f293e269a08f2d90c51e2f15a110c79f280bb437e1005d05fca71d3e82fbc"),
        (0, 68966, "42dd73a4e6d7bcc903b6033dedf72eb769f2c4eef18b3a022dfae41a6690c62b"),
        (0, 649070, "c2ddefdac32661603fe130d9b99e70cdf7fc84473f57d488d5822d0ebf7cfd0a"),
        (0, 649070, "ce63f3e826855d7a23ccbf91cc00327e7036b8088c16a74f84569a3cdbcf6f68"),
        (0, 68966, "53e555bb639467a99bca9f6d5f22cf27bddafc34ca46fbafcd71466b8d6f53bc"),
        (0, 68966, "a7d00ddb6d035a8382961e09812fd2c1a6412b9501deea5276447b323160d043"),
        (0, 68966, "b69d81731c57916432d13fa408ef4ba1479409ec0ea854c5db82db29e5aa41bb"),
        (0, 68966, "cd258b257fd4081738cf06263119638f58ded6686a37a9f340fe8184a26d4f61"),
        (0, 68966, "ff65aca20d49245261dfd3404d989e755eb903c4371da57dfa564145617707b0"),
        (0, 68966, "f0a290316a8c5ee01e796e09f8da23fe25f12960c650dd17bf663fc5ba559a06"),
        (0, 68966, "e62e568a6a4f93f65610f5bd527a84ea06b2621070711db5b3afcd1e78bba23a"),
        (0, 68966, "70a9d78e3c2857559e01d317363efc6907b22286768ff96fa5897e0a8048976f"),
        (0, 68966, "7676d3a6d749cf302efe025edc0acb5d94592e996d696ec7d48acc351d40e770"),
        (0, 68966, "035963405dcb381d3c44c81530c22c5956a800c59a6b2c90aa26c8cb8d1997ed"),
        (0, 68966, "726ad4f154c3510d43e464de08a4d252a8237f2f59352cef102c1e2d4445b543"),
        (0, 68966, "ce294162171fdce092f14cb294208aa0edcce6139ec411b82b49d207ab298f5b"),
        (0, 68966, "6bd7d542fe65d445fb7c7d094e729d8a30b2954c983848bb083b49ef9d50da2f"),
    ],
};

#[rustfmt::skip]
pub(super) static MATERIAL_PURE_BLACK: GithubSource = GithubSource {
    raw_root: "https://raw.githubusercontent.com/SullensCR/Windows-Material-Design-Cursor-V2-Dark-Hdpi-by-jepriCreations/7c643edf4a39f94c056335fb0233a041ca7aa926",
    dirs: &["cursor/pure%20black"],
    files: [
        (0, 68966, "469af8046cd6a2bd27260ab3a0815a371a4dc30cacd79d2d5f8d82b23e076c27"),
        (0, 68966, "f0540f8ef22f1d5dab1ba23bdd68269ce873e7aa1a0241ef2e7f47a76a6e402b"),
        (0, 1448584, "3e6d65b925262ddbdf85995b5cd61185a5ca63771b05448af90ced32185f9691"),
        (0, 1448572, "47b52c32a95546dc6935bbdad0ada66ee791c20a76c995e15ef7562f19bb2861"),
        (0, 68966, "53e555bb639467a99bca9f6d5f22cf27bddafc34ca46fbafcd71466b8d6f53bc"),
        (0, 68966, "a5a17953848fdfaa2003a26f3121f4e56346fcd9c2fe25013cd1beec6dfdffc9"),
        (0, 68966, "6b674795804966e3388d95c4380da693c2d8267b66a9b0d5cdcf1646bce9a919"),
        (0, 68966, "672d8676bbdb5d87efbd8cf48bf47344a0cbe32fc3dcb24342b7d6c1c311b961"),
        (0, 68966, "fca7f4f80736fcdeee5033866bcbee7e3d8696f2ff3db9d1a21d7d072bc4bf31"),
        (0, 68966, "0aba2caf8461e25edb857fc23d048528850e4e1c4e360f480d11b5c6c58c9f3c"),
        (0, 68966, "b2c875024ad24a7b0b7b2aa1327bdbd41135a2f2eaa3ee82b608a6f31fabdab5"),
        (0, 68966, "56bf4a022e4ed7635dc3ee6cb45dcfc33fb5ef1c1aad58a1d38d8ae3959cb44e"),
        (0, 68966, "bbcbd3011612c5ea0c80e8ac63e43ae32042ce69d5e25fb1e1959383036696a2"),
        (0, 68966, "2f5e0b8f0e6155e0d46e2c608f2d046772e0e153d8ab98e160f7303e185efc32"),
        (0, 68966, "7ad2c3194c00501a0e2646bd90e08ad9851088b7559896de5f97ffa4c3951b02"),
        (0, 68966, "3ab3f261fbdd4f03dad7ec505cca7d0edb2b94c9da3e89846b662a8c6e769bca"),
        (0, 68966, "6e36ea198f734ae8691103a82dc839e800f72cb1777a1ef6861bd75d3295c75f"),
    ],
};

#[rustfmt::skip]
pub(super) static TAIL_LIGHT_CANDY: GithubSource = GithubSource {
    raw_root: "https://raw.githubusercontent.com/SullensCR/Windows-11-Hdpi-Tail-Cursor-Concept-by-jepriCreations/c177f27c1fff0824f7b5c76563f3bd0eab98a9a2",
    dirs: &["light/base", "light/09.%20candy"],
    files: [
        (0, 68966, "42722efda16b96a7c657a7ffc9bc65d405fe79b755908e0d5eb0edbb61b64d14"),
        (0, 68966, "5ee41ae7112093e4b49f6079c28f6b71888be2bb7420cce27e498a85dc44f47a"),
        (1, 2552106, "e7b3f7f26b474a2bf01861885333ba5691d6e0067a75bce9ec2c6bca4efd3e4b"),
        (1, 2552106, "6c5a0225d2a5009688da9f54cf85124f5623e1c2603648e249ec80c67d021f78"),
        (0, 68966, "b411b483216e35e21ccea66a1fd474b27abd07d6fcb18e52c6a9d2f8e4961edf"),
        (0, 68966, "5d58a7bbcbebe20e401895f9fb04dcafc611d00d292e6c23f7bbea5495c5bfce"),
        (0, 68966, "529913988452d9c7a64bf580bf470200564ace29c23c715a2234cbd204f38035"),
        (0, 68966, "ebe9c09d6ff24ff16671743c875e6cae1d96d45407ace0007484d9bef7e54f2e"),
        (0, 68966, "dc8e3d619e55a856cc35d7930845913680e3d5486ebfa02c1fc45c3d1d6027bd"),
        (0, 68966, "676492186c0a33ab05a0387531e39889ef2fbce157e352f9fa89e2fe9e6fd543"),
        (0, 68966, "73b5222d87cde8328b3bbeaa6946714583e9ac071c1ebece21a4ebed85195e01"),
        (0, 68966, "1f34c163c902c6ae0cab8a95e73e5dc8a09393bc60c89f3b5b486762114a64a2"),
        (0, 68966, "c351d8906fedd62da9170cfdd309d79c889e56abdc217850f0794c48b9bde828"),
        (0, 68966, "af720f63c5c5750e8d6b969cb4b830321ea37169455bc0ec055e4b45607ffd26"),
        (0, 68966, "afc9c7e27a022f95b8bb61de306ef6b03fb52e90ffd628b43029d8c0569b33ca"),
        (0, 68966, "3a83f9e0769dab8993eefbb356df3b5cdcdedcfdb582f495ab964048dfc8ce01"),
        (0, 68966, "0bf317687eb3c7a3be7718566e099c3060a4c747113a55d11847d6db845422b0"),
    ],
};

#[rustfmt::skip]
pub(super) static TAIL_DARK_CANDY: GithubSource = GithubSource {
    raw_root: "https://raw.githubusercontent.com/SullensCR/Windows-11-Hdpi-Tail-Cursor-Concept-by-jepriCreations/c177f27c1fff0824f7b5c76563f3bd0eab98a9a2",
    dirs: &["dark/base", "dark/09.%20candy"],
    files: [
        (0, 68966, "5180468a243c8855638fe7b2a29cdb083b058387a06a4ad41be2fc0a377d076d"),
        (0, 68966, "2fe4c6dec8d413cc8500e0fed987f97c714f6c42e335f9835ad0fee297679a5d"),
        (1, 2552106, "4750d311f18b2e31f20e2418e03665d4a8a7f52923ea4c0de5c08cb40609163a"),
        (1, 2552106, "6c5a0225d2a5009688da9f54cf85124f5623e1c2603648e249ec80c67d021f78"),
        (0, 68966, "b411b483216e35e21ccea66a1fd474b27abd07d6fcb18e52c6a9d2f8e4961edf"),
        (0, 68966, "5d58a7bbcbebe20e401895f9fb04dcafc611d00d292e6c23f7bbea5495c5bfce"),
        (0, 68966, "49d71f918bf61620b30db1086fe0997ebc09fdf8f81e19dd9e82706fe1792ca3"),
        (0, 68966, "2197dd21693b00d3d4d0c35d72df4a49f012b0c5e3db96991feb98443821d4f3"),
        (0, 68966, "dc8e3d619e55a856cc35d7930845913680e3d5486ebfa02c1fc45c3d1d6027bd"),
        (0, 68966, "676492186c0a33ab05a0387531e39889ef2fbce157e352f9fa89e2fe9e6fd543"),
        (0, 68966, "73b5222d87cde8328b3bbeaa6946714583e9ac071c1ebece21a4ebed85195e01"),
        (0, 68966, "1f34c163c902c6ae0cab8a95e73e5dc8a09393bc60c89f3b5b486762114a64a2"),
        (0, 68966, "c351d8906fedd62da9170cfdd309d79c889e56abdc217850f0794c48b9bde828"),
        (0, 68966, "af720f63c5c5750e8d6b969cb4b830321ea37169455bc0ec055e4b45607ffd26"),
        (0, 68966, "d274fba08159ce2cf8a23c67813d058ccb2d97be0632a5c0cfeccec4a9cb694d"),
        (0, 68966, "ccfc22de4acb9b8951e533b5d0e04951acb17dfe571fa46bae8452bdf8169d1d"),
        (0, 68966, "d59b28cbf7bfa566602ac53d98eb1f5ba9233664c885a294a702340543ce60b6"),
    ],
};

#[rustfmt::skip]
pub(super) static CONCEPT1_DARK: GithubSource = GithubSource {
    raw_root: "https://raw.githubusercontent.com/SullensCR/Cursor-Concept-1-HDPI-By-jepriCreations/d7ea88d4ce15568ce7c0920d4e4d2da451bf45b5",
    dirs: &["dark"],
    files: [
        (0, 19275, "af8dbc604cc133b87e295d9573b4145516e4b4a8e5ec2a89341264719c73cc0a"),
        (0, 25645, "19722fed6cc161c3ae3cffd349831fa252ba868efc73b1a55b02299d455451f8"),
        (0, 290036, "4ab486e1c6a004bf9a7c0f190e96dd90f8ed67f8108f47789b334db23cf1f804"),
        (0, 292560, "6423e8e507d0d624619d9f41ada5947e7687b029415628b1244e0497274f92b1"),
        (0, 16236, "5682484b2d12e7f02f5fdfc21ae6a8454d54beb9914e1167fd7ddaa2e88be61b"),
        (0, 16493, "d42d7cf5ed5d8fe63cebcd132183e9c762252ac98037379346b43323b528bce8"),
        (0, 17003, "b992c3efac4c2271fa002a8bdf54032d57285842d641591bb98cae2edce878e8"),
        (0, 24760, "b67956492c2c41de5cd2de37e4ac42f13ecbbfeaabacf1a76b603076371107ac"),
        (0, 21389, "ef2bb84d06bc8368625684fede0a6b771a841e7cb9020a87fe76b6cb844906c2"),
        (0, 21338, "94e8f01545f42d7e9a6191198ae300ffe0c0d6974dcf801fc245d6628fdec67e"),
        (0, 20579, "fcd833ea7e3125dad0fa63412aff6992b49b5adc600a273ad91ce6abcb56b228"),
        (0, 20537, "85902402f444b0748403fc79fac4704ccafaefa118d4d3f1253b0a22b9b0d5d6"),
        (0, 25822, "36c6fe9fa7d344945f1fd52d8936caa7f9ca94f3cfc06afa9b5f5895a900ad6a"),
        (0, 18599, "a8571b3f65d831d33ef54eb2c8d2ac9588961312de80b7c1e8e45b27f3158115"),
        (0, 20787, "4f70f3fe32bf30591bc832608978f6dee5837d956eb44153da11eeec45173c79"),
        (0, 32162, "13d04f7d2223e880a01d1a3724af62c00a246d39d57700e1123e9610754b509f"),
        (0, 31949, "63898258112fab6a9d9dcd75a2d383b39864f8bb56f1734e982e2fbd17123f81"),
    ],
};

#[rustfmt::skip]
pub(super) static CONCEPT1_LIGHT: GithubSource = GithubSource {
    raw_root: "https://raw.githubusercontent.com/SullensCR/Cursor-Concept-1-HDPI-By-jepriCreations/d7ea88d4ce15568ce7c0920d4e4d2da451bf45b5",
    dirs: &["light"],
    files: [
        (0, 18809, "defb7935c247b1ec37dd57e0c918b968f8091139f9b21ea54c0ee4c331f4f0d6"),
        (0, 23724, "59629b75ffbb41e93811f52a23a8e88b3686beef42473ed80aa8cf67e2c9b071"),
        (0, 278282, "ed3d40ce81d29f44962635f4c5d8c44648ee5e3b69d85396bc73a8906bae54fd"),
        (0, 281894, "28b08cdcdc24b1df19f6130c785707d82819d3a3cd0bc20bcbca5540338458ea"),
        (0, 16236, "5682484b2d12e7f02f5fdfc21ae6a8454d54beb9914e1167fd7ddaa2e88be61b"),
        (0, 16493, "d42d7cf5ed5d8fe63cebcd132183e9c762252ac98037379346b43323b528bce8"),
        (0, 16927, "8dfa5ce1ac695cd1ef80706779e690b5b8e4353a4ac08ca025c2ba784967ae30"),
        (0, 22957, "8c7b3a87d642b00ccecb8a403b6ff4b2f65bbfaba0b4e33994351bba13f90a2d"),
        (0, 19808, "f7619fb9d08048670b862efe537acf34de75b098c5b03efa9fd1b3eb8458d08e"),
        (0, 19553, "78e384280ebc84911cf7ee17cc347e5488463a767ae194b7e6a0fd017eb51ea2"),
        (0, 19525, "7eea1bca0fef256dd95957ec3d42785c6ba0706e9d8b4be9f4bd4fea44a12cc6"),
        (0, 19515, "9bea2ee249a76221fb342274c19acb7aedc01b53435cc2dc8b8b8ba26943deb2"),
        (0, 22583, "8b13facefef8e04c3f497062d578b2faba8f9b4af57feb31ed80cccf35aca8a4"),
        (0, 18158, "2c24b07d20fedfb7644387359d98842250e6e2d418dbf53c9407151d784568a5"),
        (0, 19829, "c12d4ff95fc056a7cb20171a433615098368813b9112e586b5a773532fae9341"),
        (0, 26433, "fa9718e26261f4c1b3616054fae3a774135849c97f95fd34aa78473b412fe138"),
        (0, 26167, "ad6a88954a308e862998185d616487710500e45b4c439f1a23a2a5d2a814dd81"),
    ],
};

#[rustfmt::skip]
pub(super) static ARCHIVE_W11_CONCEPT_DARK: GithubSource = GithubSource {
    raw_root: "https://raw.githubusercontent.com/udaykamboj/jepriCreations-Unofficial-Archive-Repo/5a39e1b24134f2cbe6a77db221179bc4431777f3",
    dirs: &["windows_11_cursors_concept/dark"],
    files: [
        (0, 4286, "5d147ce99167984abbcddcbd90ffe8679fb267399165029d3d367d99cbd11141"),
        (0, 4286, "26eb9dda636d263b3c6a3faf70c78e64a5aa965fee58e4690571db197754eec0"),
        (0, 158946, "57ffec7713ad16ac420f58727408d34216e6e380263fc93185280207a647653b"),
        (0, 158946, "03b7a3b60ceac788590fdba988b98e9afd7187d11774028c27b3d10154798689"),
        (0, 4286, "a08c66bd12308f80c10de35b4485aadd700adc2ee15bef382e100ea702f6e6d2"),
        (0, 4286, "ee1a28b7c3af8006dff6a4235445a8bad4dd5e862e2cad24315f7570486a1c96"),
        (0, 4286, "de4baa56f6be8b705488f329317e456fa1de61a61dc3ccdf4a8a016a1b6c4b18"),
        (0, 4286, "b6a99a68e18f99a4028cf26866bb720af8eca3eff88f5f09aeb9cf2dc315b88e"),
        (0, 4286, "84df1b52cb545ca44c7090a86966b6e6db2bb7a779134e5d8125852ef70dded7"),
        (0, 4286, "cdae60f414d026aaf31124f1ce2fb2ec6f3978d0900a4876831371516a2df7e0"),
        (0, 4286, "a51830dac80c82e5c9c4dc690aa78cc58f0d3ba650b90ec0801454eaf3fb97d7"),
        (0, 4286, "5ac7d4323e15133d442229877215e1a84af2c32ff53f7b4156fa3f22d103f8b4"),
        (0, 4286, "2c1a145366a08f353d67fda560edf3337f5b240f49db7a8810fb4d699d4b52cd"),
        (0, 4286, "07c7b1abc812f0342390d9b3cb6878c8a498c385a7b253f02355dc7308bf2eb3"),
        (0, 4286, "591238c23d1957c7d702eaaaa3536e71bfde6bf0d75dcd46639d938f7f73ee1e"),
        (0, 4286, "c6c3ebfac60d8252f8669b606e08eb0666cbba5df02126d7f2d462046be355b8"),
        (0, 4286, "66a167913634d0d85c0f137cb360cc590530c4f1e56b57870eebdf53ff9f126f"),
    ],
};

#[rustfmt::skip]
pub(super) static ARCHIVE_W11_CONCEPT_LIGHT: GithubSource = GithubSource {
    raw_root: "https://raw.githubusercontent.com/udaykamboj/jepriCreations-Unofficial-Archive-Repo/5a39e1b24134f2cbe6a77db221179bc4431777f3",
    dirs: &["windows_11_cursors_concept/light"],
    files: [
        (0, 4286, "990e755766b62c328bb109b5bcc60f2b7d36d5938f36557cc8c52eb2c0d04ddd"),
        (0, 4286, "3e179ab80116488ee721e85b48174cdcf0d7e081f5e81afd2a46cc6489fc6a45"),
        (0, 158946, "e5c1d8b253cf5cce4a76fb00b72e8cd5f2b11258448694b18c4522dd6bebca77"),
        (0, 158946, "d2af72fd66fcb7746ae8fc204aa6a1614ef4926f64b642a2a3c6496ba8ed8b4f"),
        (0, 4286, "a08c66bd12308f80c10de35b4485aadd700adc2ee15bef382e100ea702f6e6d2"),
        (0, 4286, "ee1a28b7c3af8006dff6a4235445a8bad4dd5e862e2cad24315f7570486a1c96"),
        (0, 4286, "2cd792e7502ff929ad9664b09c3017bcfac4273ebc605ca12d6b070cb489a0cb"),
        (0, 4286, "f273c044fac2a109bcbaa81698df3a2b99e8dddaae9c079a8a3d2e02d7bf8d88"),
        (0, 4286, "e1a0b2cd84378a00c9236f3a871eb6b54eb00cf602fc092415942445200dc0e4"),
        (0, 4286, "9483b724b20847a697c52cc3d907985e3bfbcd7a67a6502707bb485fff0aab23"),
        (0, 4286, "1a171564470e583591365d1b152f71cd3432f33120cb02ce35c043f1ae87fa12"),
        (0, 4286, "64195c4345e9689cd7b52b85b8fbc6c07312459004cca6921640167698d6f536"),
        (0, 4286, "1840367f6f1d70056f1421d481acb85411d38c46dc8f8bb0abb3f18475dd520b"),
        (0, 4286, "e0f86f0ce48977691e14205013f705a563eba92f603ea04af3ddb840e33b05f2"),
        (0, 4286, "31b6c509dd4e0ebf590b9419434efa28275edc61fbabc7701611872279277dc6"),
        (0, 4286, "477df1d22aac7cdff1f5ce3fd0df506424b7971e0acba7139d0e38944a8d182f"),
        (0, 4286, "acda61e4068fd30001df43072525d7900df1e01cb2fa48e21551a314d5f524bc"),
    ],
};

#[rustfmt::skip]
pub(super) static ARCHIVE_W11_TAIL_DARK: GithubSource = GithubSource {
    raw_root: "https://raw.githubusercontent.com/udaykamboj/jepriCreations-Unofficial-Archive-Repo/5a39e1b24134f2cbe6a77db221179bc4431777f3",
    dirs: &["windows_11_tail_cursor_concept/dark"],
    files: [
        (0, 4286, "d93c7d5ad5f9b1fd6d4e1266e68aaeb04f832a38f2ebceeda678078b062d3baa"),
        (0, 4286, "69fcce2e2a2603860b02fed8d2340b6e6d15b207922a28240093981669d6c2b8"),
        (0, 158946, "fba37d457097055c3a8f867f3499ac60901e62cb1c4f0cf24568ab553647d07b"),
        (0, 158946, "03b7a3b60ceac788590fdba988b98e9afd7187d11774028c27b3d10154798689"),
        (0, 4286, "a08c66bd12308f80c10de35b4485aadd700adc2ee15bef382e100ea702f6e6d2"),
        (0, 4286, "ee1a28b7c3af8006dff6a4235445a8bad4dd5e862e2cad24315f7570486a1c96"),
        (0, 4286, "de4baa56f6be8b705488f329317e456fa1de61a61dc3ccdf4a8a016a1b6c4b18"),
        (0, 4286, "b6a99a68e18f99a4028cf26866bb720af8eca3eff88f5f09aeb9cf2dc315b88e"),
        (0, 4286, "84df1b52cb545ca44c7090a86966b6e6db2bb7a779134e5d8125852ef70dded7"),
        (0, 4286, "cdae60f414d026aaf31124f1ce2fb2ec6f3978d0900a4876831371516a2df7e0"),
        (0, 4286, "a51830dac80c82e5c9c4dc690aa78cc58f0d3ba650b90ec0801454eaf3fb97d7"),
        (0, 4286, "5ac7d4323e15133d442229877215e1a84af2c32ff53f7b4156fa3f22d103f8b4"),
        (0, 4286, "2c1a145366a08f353d67fda560edf3337f5b240f49db7a8810fb4d699d4b52cd"),
        (0, 4286, "07c7b1abc812f0342390d9b3cb6878c8a498c385a7b253f02355dc7308bf2eb3"),
        (0, 4286, "591238c23d1957c7d702eaaaa3536e71bfde6bf0d75dcd46639d938f7f73ee1e"),
        (0, 4286, "c6c3ebfac60d8252f8669b606e08eb0666cbba5df02126d7f2d462046be355b8"),
        (0, 4286, "66a167913634d0d85c0f137cb360cc590530c4f1e56b57870eebdf53ff9f126f"),
    ],
};

#[rustfmt::skip]
pub(super) static ARCHIVE_W11_TAIL_LIGHT: GithubSource = GithubSource {
    raw_root: "https://raw.githubusercontent.com/udaykamboj/jepriCreations-Unofficial-Archive-Repo/5a39e1b24134f2cbe6a77db221179bc4431777f3",
    dirs: &["windows_11_tail_cursor_concept/light"],
    files: [
        (0, 4286, "24bcfdca46b376ae68f72132fb846093e2f8e4ad6af448a57d680ca6a78e50b7"),
        (0, 4286, "121b9320982a1792e954df51bd7ac276b4bc93d58f80fc928382d9bfc44f3349"),
        (0, 158946, "2aa0b208613bc3b2252a68b3b294d528058d1c9e4affa7c977ce1a75d6b87158"),
        (0, 158946, "d2af72fd66fcb7746ae8fc204aa6a1614ef4926f64b642a2a3c6496ba8ed8b4f"),
        (0, 4286, "a08c66bd12308f80c10de35b4485aadd700adc2ee15bef382e100ea702f6e6d2"),
        (0, 4286, "ee1a28b7c3af8006dff6a4235445a8bad4dd5e862e2cad24315f7570486a1c96"),
        (0, 4286, "2cd792e7502ff929ad9664b09c3017bcfac4273ebc605ca12d6b070cb489a0cb"),
        (0, 4286, "f273c044fac2a109bcbaa81698df3a2b99e8dddaae9c079a8a3d2e02d7bf8d88"),
        (0, 4286, "e1a0b2cd84378a00c9236f3a871eb6b54eb00cf602fc092415942445200dc0e4"),
        (0, 4286, "9483b724b20847a697c52cc3d907985e3bfbcd7a67a6502707bb485fff0aab23"),
        (0, 4286, "1a171564470e583591365d1b152f71cd3432f33120cb02ce35c043f1ae87fa12"),
        (0, 4286, "64195c4345e9689cd7b52b85b8fbc6c07312459004cca6921640167698d6f536"),
        (0, 4286, "1840367f6f1d70056f1421d481acb85411d38c46dc8f8bb0abb3f18475dd520b"),
        (0, 4286, "e0f86f0ce48977691e14205013f705a563eba92f603ea04af3ddb840e33b05f2"),
        (0, 4286, "31b6c509dd4e0ebf590b9419434efa28275edc61fbabc7701611872279277dc6"),
        (0, 4286, "477df1d22aac7cdff1f5ce3fd0df506424b7971e0acba7139d0e38944a8d182f"),
        (0, 4286, "acda61e4068fd30001df43072525d7900df1e01cb2fa48e21551a314d5f524bc"),
    ],
};

#[rustfmt::skip]
pub(super) static ANTIDEN_SIERRA_SHADOW: GithubSource = GithubSource {
    raw_root: "https://raw.githubusercontent.com/antiden/macOS-cursors-for-Windows/baaea95bef945a26084edb2cc205675eb7548b03",
    dirs: &["1.%20Sierra%20and%20newer/2.%20With%20Shadow/1.%20Normal"],
    files: [
        (0, 142491, "8fb33998d08656afb04a92f38acba9cbc104f908202591ff9d5a1e3e8d5fe3c8"),
        (0, 144524, "924de9982f77029297c8a8bf378ee477fc3fac0cba19be2c1fd9c2dd10c0334c"),
        (0, 1082532, "7aefbcafc08364d86d5a45b126b311ae930983873f9345ed3ac7a93240359271"),
        (0, 1082532, "84d49b65f13aba1b9e2f7d332a6167874b04056d5babaf5e14912dc0e943592c"),
        (0, 142054, "f13588a4209b39e45f37e1ec1eb3f7a519eee9778f662db2e004c9f819dedda4"),
        (0, 145802, "5428efc5ea082ab9c0c513cc98e63b60321686359ac9086649630cb29f26c3a2"),
        (0, 146023, "c80990a0a8973eb6a945825e0c83e31da5b004e1d78f35b97f2d561a3fc3f371"),
        (0, 155047, "2740b518764926d8727f36c9484b39191a988a39405cfd5f901e594f8299caa8"),
        (0, 143446, "5a1acf464aa967c166e4a84afd848e2a26eefc1a07a7ac7cbf643b55ee78eec6"),
        (0, 144438, "56c32803f396bc493ad45d90c99bab476a470a52a0cd600a179776c94b9595d9"),
        (0, 143337, "71650d0779512dced6e1fd9a3f048bf5a90320c40957dc13807394056437c0eb"),
        (0, 143336, "a7a5ef81b3a73d035c920547655123913e7398e834d7617916f99205ffcd7207"),
        (0, 143222, "c19c0dacb3187336213d719e2e6870f198cc29b6b2c421b6191d550dc16edf1f"),
        (0, 145893, "c120e2b22b19e0e43cfad4b11c986b008c142f4e074ce04b47ab31dfc3d47e8f"),
        (0, 145407, "29edafbc0e3b06370f2876e96b47032b498aa9b00b36a6ef4a263d472161be4f"),
        (0, 147303, "52f9a7776d0c46c00c29cd8e6df8ecf3251d4ce84b716a987d6c530e992a11e3"),
        (0, 146880, "833bc3290a4e49117ad7b0224c4e6b3d62337c1217438f4eeebe959dc458acb6"),
    ],
};

#[rustfmt::skip]
pub(super) static BREEZE_PLASMA6: GithubSource = GithubSource {
    raw_root: "https://raw.githubusercontent.com/black7375/Breeze-Cursors-for-Windows/40c80c200daa6bfe9cc3dd91333b89b8c32c431d",
    dirs: &["plasma6/output"],
    files: [
        (0, 15454, "e60226af4dbfe5c9c20e71034010df2894e37b8cf4d978041b3b82623a4017f9"),
        (0, 34633, "8f35c4ff235fcef79fdba0b02d45ba8643b986b246a03dd3704947484e58b2be"),
        (0, 821530, "3be632317027ee9bd0fa387547290a1c50da8a89809f298ad3b08ad531c75e50"),
        (0, 902670, "9622cccbaf8a38ce7582e78f1ed7e8fcabdff1e03bd78bfb2fe5936c05599023"),
        (0, 15626, "2befe8e65aebd70a5fb729661fcb4154173e8e6951413c142d1fa2cd63b1dc1a"),
        (0, 12251, "7847f15be5fa38e29690d1a696130bec4b8a896d416cf48d64e9dd269d778f55"),
        (0, 12728, "3e4ab49ffe712a5b614df3a2f648030791a60a222105c2e8c082ac9f889bad31"),
        (0, 39283, "51f66867bb57c55c7e9163c7ba8cf245592436ed833803f8a709cb68eb35ff7b"),
        (0, 26686, "ba6559992ee759bd6c829a4df53aad00a4994a9ca262f0ad80db9e9a28e23c47"),
        (0, 26137, "870140810b51c4bd8cfdf44fd6d5546cc35552b306f326a8d6e151d106dd7bee"),
        (0, 27242, "a6d9624ce046a2eb8ba8ed079f5183508162cc37b1312cd401c0424a2dfd6f25"),
        (0, 27441, "5c59709cadb7e1a4e999df0784a99fe13eccf6555c734de1af8968451c2bb59b"),
        (0, 41753, "805f37d336e75d9f35c4d13284b8a94df2bab789da93fdbaa172271c582dc104"),
        (0, 22310, "bb5e08fac831a76332e7d430ba644b931921a8bad15e24d6dc695d22fd02b9b5"),
        (0, 23409, "842e84415c5f8f4c4a8f0be2360b2ea3b6634fdec6bebb7d31970059cd4024e1"),
        (0, 15454, "e60226af4dbfe5c9c20e71034010df2894e37b8cf4d978041b3b82623a4017f9"),
        (0, 15454, "e60226af4dbfe5c9c20e71034010df2894e37b8cf4d978041b3b82623a4017f9"),
    ],
};
