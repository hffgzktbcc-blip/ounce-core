// AES / Rijndael-128 from Daemen and Rijmen, "AES Proposal: Rijndael" (1998) and "The Design of Rijndael" (2002), as standardised in
// FIPS 197, written from the standard's description: the S-box is computed (inverse in GF(2^8) plus the affine map), not tabulated.
// Compares every value of FIPS 197 Appendix C (the cipher, inverse cipher and equivalent inverse cipher intermediates for AES-128,
// AES-192 and AES-256: 558 states, copied as data by script). Exit 0 only if all match.
#include "ounce_paper.h"

// FIPS 197 Appendix C: key and plaintext of C.1 (AES-128), C.2 (AES-192), C.3 (AES-256)
static const char *KEYS[3] = {
    "000102030405060708090a0b0c0d0e0f",
    "000102030405060708090a0b0c0d0e0f1011121314151617",
    "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
};
static const char *PLAIN = "00112233445566778899aabbccddeeff";
// every 'round[r].tag' line of C.1-C.3: {key size index, 0 cipher / 1 inverse cipher / 2 equivalent inverse cipher, round, tag, 16 bytes}
static const struct { u8 ks, sec, round; const char *tag; const char *hex; } REC[] = {
    {0, 0, 0, "input", "00112233445566778899aabbccddeeff"},
    {0, 0, 0, "k_sch", "000102030405060708090a0b0c0d0e0f"},
    {0, 0, 1, "start", "00102030405060708090a0b0c0d0e0f0"},
    {0, 0, 1, "s_box", "63cab7040953d051cd60e0e7ba70e18c"},
    {0, 0, 1, "s_row", "6353e08c0960e104cd70b751bacad0e7"},
    {0, 0, 1, "m_col", "5f72641557f5bc92f7be3b291db9f91a"},
    {0, 0, 1, "k_sch", "d6aa74fdd2af72fadaa678f1d6ab76fe"},
    {0, 0, 2, "start", "89d810e8855ace682d1843d8cb128fe4"},
    {0, 0, 2, "s_box", "a761ca9b97be8b45d8ad1a611fc97369"},
    {0, 0, 2, "s_row", "a7be1a6997ad739bd8c9ca451f618b61"},
    {0, 0, 2, "m_col", "ff87968431d86a51645151fa773ad009"},
    {0, 0, 2, "k_sch", "b692cf0b643dbdf1be9bc5006830b3fe"},
    {0, 0, 3, "start", "4915598f55e5d7a0daca94fa1f0a63f7"},
    {0, 0, 3, "s_box", "3b59cb73fcd90ee05774222dc067fb68"},
    {0, 0, 3, "s_row", "3bd92268fc74fb735767cbe0c0590e2d"},
    {0, 0, 3, "m_col", "4c9c1e66f771f0762c3f868e534df256"},
    {0, 0, 3, "k_sch", "b6ff744ed2c2c9bf6c590cbf0469bf41"},
    {0, 0, 4, "start", "fa636a2825b339c940668a3157244d17"},
    {0, 0, 4, "s_box", "2dfb02343f6d12dd09337ec75b36e3f0"},
    {0, 0, 4, "s_row", "2d6d7ef03f33e334093602dd5bfb12c7"},
    {0, 0, 4, "m_col", "6385b79ffc538df997be478e7547d691"},
    {0, 0, 4, "k_sch", "47f7f7bc95353e03f96c32bcfd058dfd"},
    {0, 0, 5, "start", "247240236966b3fa6ed2753288425b6c"},
    {0, 0, 5, "s_box", "36400926f9336d2d9fb59d23c42c3950"},
    {0, 0, 5, "s_row", "36339d50f9b539269f2c092dc4406d23"},
    {0, 0, 5, "m_col", "f4bcd45432e554d075f1d6c51dd03b3c"},
    {0, 0, 5, "k_sch", "3caaa3e8a99f9deb50f3af57adf622aa"},
    {0, 0, 6, "start", "c81677bc9b7ac93b25027992b0261996"},
    {0, 0, 6, "s_box", "e847f56514dadde23f77b64fe7f7d490"},
    {0, 0, 6, "s_row", "e8dab6901477d4653ff7f5e2e747dd4f"},
    {0, 0, 6, "m_col", "9816ee7400f87f556b2c049c8e5ad036"},
    {0, 0, 6, "k_sch", "5e390f7df7a69296a7553dc10aa31f6b"},
    {0, 0, 7, "start", "c62fe109f75eedc3cc79395d84f9cf5d"},
    {0, 0, 7, "s_box", "b415f8016858552e4bb6124c5f998a4c"},
    {0, 0, 7, "s_row", "b458124c68b68a014b99f82e5f15554c"},
    {0, 0, 7, "m_col", "c57e1c159a9bd286f05f4be098c63439"},
    {0, 0, 7, "k_sch", "14f9701ae35fe28c440adf4d4ea9c026"},
    {0, 0, 8, "start", "d1876c0f79c4300ab45594add66ff41f"},
    {0, 0, 8, "s_box", "3e175076b61c04678dfc2295f6a8bfc0"},
    {0, 0, 8, "s_row", "3e1c22c0b6fcbf768da85067f6170495"},
    {0, 0, 8, "m_col", "baa03de7a1f9b56ed5512cba5f414d23"},
    {0, 0, 8, "k_sch", "47438735a41c65b9e016baf4aebf7ad2"},
    {0, 0, 9, "start", "fde3bad205e5d0d73547964ef1fe37f1"},
    {0, 0, 9, "s_box", "5411f4b56bd9700e96a0902fa1bb9aa1"},
    {0, 0, 9, "s_row", "54d990a16ba09ab596bbf40ea111702f"},
    {0, 0, 9, "m_col", "e9f74eec023020f61bf2ccf2353c21c7"},
    {0, 0, 9, "k_sch", "549932d1f08557681093ed9cbe2c974e"},
    {0, 0, 10, "start", "bd6e7c3df2b5779e0b61216e8b10b689"},
    {0, 0, 10, "s_box", "7a9f102789d5f50b2beffd9f3dca4ea7"},
    {0, 0, 10, "s_row", "7ad5fda789ef4e272bca100b3d9ff59f"},
    {0, 0, 10, "k_sch", "13111d7fe3944a17f307a78b4d2b30c5"},
    {0, 0, 10, "output", "69c4e0d86a7b0430d8cdb78070b4c55a"},
    {0, 1, 0, "iinput", "69c4e0d86a7b0430d8cdb78070b4c55a"},
    {0, 1, 0, "ik_sch", "13111d7fe3944a17f307a78b4d2b30c5"},
    {0, 1, 1, "istart", "7ad5fda789ef4e272bca100b3d9ff59f"},
    {0, 1, 1, "is_row", "7a9f102789d5f50b2beffd9f3dca4ea7"},
    {0, 1, 1, "is_box", "bd6e7c3df2b5779e0b61216e8b10b689"},
    {0, 1, 1, "ik_sch", "549932d1f08557681093ed9cbe2c974e"},
    {0, 1, 1, "ik_add", "e9f74eec023020f61bf2ccf2353c21c7"},
    {0, 1, 2, "istart", "54d990a16ba09ab596bbf40ea111702f"},
    {0, 1, 2, "is_row", "5411f4b56bd9700e96a0902fa1bb9aa1"},
    {0, 1, 2, "is_box", "fde3bad205e5d0d73547964ef1fe37f1"},
    {0, 1, 2, "ik_sch", "47438735a41c65b9e016baf4aebf7ad2"},
    {0, 1, 2, "ik_add", "baa03de7a1f9b56ed5512cba5f414d23"},
    {0, 1, 3, "istart", "3e1c22c0b6fcbf768da85067f6170495"},
    {0, 1, 3, "is_row", "3e175076b61c04678dfc2295f6a8bfc0"},
    {0, 1, 3, "is_box", "d1876c0f79c4300ab45594add66ff41f"},
    {0, 1, 3, "ik_sch", "14f9701ae35fe28c440adf4d4ea9c026"},
    {0, 1, 3, "ik_add", "c57e1c159a9bd286f05f4be098c63439"},
    {0, 1, 4, "istart", "b458124c68b68a014b99f82e5f15554c"},
    {0, 1, 4, "is_row", "b415f8016858552e4bb6124c5f998a4c"},
    {0, 1, 4, "is_box", "c62fe109f75eedc3cc79395d84f9cf5d"},
    {0, 1, 4, "ik_sch", "5e390f7df7a69296a7553dc10aa31f6b"},
    {0, 1, 4, "ik_add", "9816ee7400f87f556b2c049c8e5ad036"},
    {0, 1, 5, "istart", "e8dab6901477d4653ff7f5e2e747dd4f"},
    {0, 1, 5, "is_row", "e847f56514dadde23f77b64fe7f7d490"},
    {0, 1, 5, "is_box", "c81677bc9b7ac93b25027992b0261996"},
    {0, 1, 5, "ik_sch", "3caaa3e8a99f9deb50f3af57adf622aa"},
    {0, 1, 5, "ik_add", "f4bcd45432e554d075f1d6c51dd03b3c"},
    {0, 1, 6, "istart", "36339d50f9b539269f2c092dc4406d23"},
    {0, 1, 6, "is_row", "36400926f9336d2d9fb59d23c42c3950"},
    {0, 1, 6, "is_box", "247240236966b3fa6ed2753288425b6c"},
    {0, 1, 6, "ik_sch", "47f7f7bc95353e03f96c32bcfd058dfd"},
    {0, 1, 6, "ik_add", "6385b79ffc538df997be478e7547d691"},
    {0, 1, 7, "istart", "2d6d7ef03f33e334093602dd5bfb12c7"},
    {0, 1, 7, "is_row", "2dfb02343f6d12dd09337ec75b36e3f0"},
    {0, 1, 7, "is_box", "fa636a2825b339c940668a3157244d17"},
    {0, 1, 7, "ik_sch", "b6ff744ed2c2c9bf6c590cbf0469bf41"},
    {0, 1, 7, "ik_add", "4c9c1e66f771f0762c3f868e534df256"},
    {0, 1, 8, "istart", "3bd92268fc74fb735767cbe0c0590e2d"},
    {0, 1, 8, "is_row", "3b59cb73fcd90ee05774222dc067fb68"},
    {0, 1, 8, "is_box", "4915598f55e5d7a0daca94fa1f0a63f7"},
    {0, 1, 8, "ik_sch", "b692cf0b643dbdf1be9bc5006830b3fe"},
    {0, 1, 8, "ik_add", "ff87968431d86a51645151fa773ad009"},
    {0, 1, 9, "istart", "a7be1a6997ad739bd8c9ca451f618b61"},
    {0, 1, 9, "is_row", "a761ca9b97be8b45d8ad1a611fc97369"},
    {0, 1, 9, "is_box", "89d810e8855ace682d1843d8cb128fe4"},
    {0, 1, 9, "ik_sch", "d6aa74fdd2af72fadaa678f1d6ab76fe"},
    {0, 1, 9, "ik_add", "5f72641557f5bc92f7be3b291db9f91a"},
    {0, 1, 10, "istart", "6353e08c0960e104cd70b751bacad0e7"},
    {0, 1, 10, "is_row", "63cab7040953d051cd60e0e7ba70e18c"},
    {0, 1, 10, "is_box", "00102030405060708090a0b0c0d0e0f0"},
    {0, 1, 10, "ik_sch", "000102030405060708090a0b0c0d0e0f"},
    {0, 1, 10, "ioutput", "00112233445566778899aabbccddeeff"},
    {0, 2, 0, "iinput", "69c4e0d86a7b0430d8cdb78070b4c55a"},
    {0, 2, 0, "ik_sch", "13111d7fe3944a17f307a78b4d2b30c5"},
    {0, 2, 1, "istart", "7ad5fda789ef4e272bca100b3d9ff59f"},
    {0, 2, 1, "is_box", "bdb52189f261b63d0b107c9e8b6e776e"},
    {0, 2, 1, "is_row", "bd6e7c3df2b5779e0b61216e8b10b689"},
    {0, 2, 1, "im_col", "4773b91ff72f354361cb018ea1e6cf2c"},
    {0, 2, 1, "ik_sch", "13aa29be9c8faff6f770f58000f7bf03"},
    {0, 2, 2, "istart", "54d990a16ba09ab596bbf40ea111702f"},
    {0, 2, 2, "is_box", "fde596f1054737d235febad7f1e3d04e"},
    {0, 2, 2, "is_row", "fde3bad205e5d0d73547964ef1fe37f1"},
    {0, 2, 2, "im_col", "2d7e86a339d9393ee6570a1101904e16"},
    {0, 2, 2, "ik_sch", "1362a4638f2586486bff5a76f7874a83"},
    {0, 2, 3, "istart", "3e1c22c0b6fcbf768da85067f6170495"},
    {0, 2, 3, "is_box", "d1c4941f7955f40fb46f6c0ad68730ad"},
    {0, 2, 3, "is_row", "d1876c0f79c4300ab45594add66ff41f"},
    {0, 2, 3, "im_col", "39daee38f4f1a82aaf432410c36d45b9"},
    {0, 2, 3, "ik_sch", "8d82fc749c47222be4dadc3e9c7810f5"},
    {0, 2, 4, "istart", "b458124c68b68a014b99f82e5f15554c"},
    {0, 2, 4, "is_box", "c65e395df779cf09ccf9e1c3842fed5d"},
    {0, 2, 4, "is_row", "c62fe109f75eedc3cc79395d84f9cf5d"},
    {0, 2, 4, "im_col", "9a39bf1d05b20a3a476a0bf79fe51184"},
    {0, 2, 4, "ik_sch", "72e3098d11c5de5f789dfe1578a2cccb"},
    {0, 2, 5, "istart", "e8dab6901477d4653ff7f5e2e747dd4f"},
    {0, 2, 5, "is_box", "c87a79969b0219bc2526773bb016c992"},
    {0, 2, 5, "is_row", "c81677bc9b7ac93b25027992b0261996"},
    {0, 2, 5, "im_col", "18f78d779a93eef4f6742967c47f5ffd"},
    {0, 2, 5, "ik_sch", "2ec410276326d7d26958204a003f32de"},
    {0, 2, 6, "istart", "36339d50f9b539269f2c092dc4406d23"},
    {0, 2, 6, "is_box", "2466756c69d25b236e4240fa8872b332"},
    {0, 2, 6, "is_row", "247240236966b3fa6ed2753288425b6c"},
    {0, 2, 6, "im_col", "85cf8bf472d124c10348f545329c0053"},
    {0, 2, 6, "ik_sch", "a8a2f5044de2c7f50a7ef79869671294"},
    {0, 2, 7, "istart", "2d6d7ef03f33e334093602dd5bfb12c7"},
    {0, 2, 7, "is_box", "fab38a1725664d2840246ac957633931"},
    {0, 2, 7, "is_row", "fa636a2825b339c940668a3157244d17"},
    {0, 2, 7, "im_col", "fc1fc1f91934c98210fbfb8da340eb21"},
    {0, 2, 7, "ik_sch", "c7c6e391e54032f1479c306d6319e50c"},
    {0, 2, 8, "istart", "3bd92268fc74fb735767cbe0c0590e2d"},
    {0, 2, 8, "is_box", "49e594f755ca638fda0a59a01f15d7fa"},
    {0, 2, 8, "is_row", "4915598f55e5d7a0daca94fa1f0a63f7"},
    {0, 2, 8, "im_col", "076518f0b52ba2fb7a15c8d93be45e00"},
    {0, 2, 8, "ik_sch", "a0db02992286d160a2dc029c2485d561"},
    {0, 2, 9, "istart", "a7be1a6997ad739bd8c9ca451f618b61"},
    {0, 2, 9, "is_box", "895a43e485188fe82d121068cbd8ced8"},
    {0, 2, 9, "is_row", "89d810e8855ace682d1843d8cb128fe4"},
    {0, 2, 9, "im_col", "ef053f7c8b3d32fd4d2a64ad3c93071a"},
    {0, 2, 9, "ik_sch", "8c56dff0825dd3f9805ad3fc8659d7fd"},
    {0, 2, 10, "istart", "6353e08c0960e104cd70b751bacad0e7"},
    {0, 2, 10, "is_box", "0050a0f04090e03080d02070c01060b0"},
    {0, 2, 10, "is_row", "00102030405060708090a0b0c0d0e0f0"},
    {0, 2, 10, "ik_sch", "000102030405060708090a0b0c0d0e0f"},
    {0, 2, 10, "ioutput", "00112233445566778899aabbccddeeff"},
    {1, 0, 0, "input", "00112233445566778899aabbccddeeff"},
    {1, 0, 0, "k_sch", "000102030405060708090a0b0c0d0e0f"},
    {1, 0, 1, "start", "00102030405060708090a0b0c0d0e0f0"},
    {1, 0, 1, "s_box", "63cab7040953d051cd60e0e7ba70e18c"},
    {1, 0, 1, "s_row", "6353e08c0960e104cd70b751bacad0e7"},
    {1, 0, 1, "m_col", "5f72641557f5bc92f7be3b291db9f91a"},
    {1, 0, 1, "k_sch", "10111213141516175846f2f95c43f4fe"},
    {1, 0, 2, "start", "4f63760643e0aa85aff8c9d041fa0de4"},
    {1, 0, 2, "s_box", "84fb386f1ae1ac977941dd70832dd769"},
    {1, 0, 2, "s_row", "84e1dd691a41d76f792d389783fbac70"},
    {1, 0, 2, "m_col", "9f487f794f955f662afc86abd7f1ab29"},
    {1, 0, 2, "k_sch", "544afef55847f0fa4856e2e95c43f4fe"},
    {1, 0, 3, "start", "cb02818c17d2af9c62aa64428bb25fd7"},
    {1, 0, 3, "s_box", "1f770c64f0b579deaaac432c3d37cf0e"},
    {1, 0, 3, "s_row", "1fb5430ef0accf64aa370cde3d77792c"},
    {1, 0, 3, "m_col", "b7a53ecbbf9d75a0c40efc79b674cc11"},
    {1, 0, 3, "k_sch", "40f949b31cbabd4d48f043b810b7b342"},
    {1, 0, 4, "start", "f75c7778a327c8ed8cfebfc1a6c37f53"},
    {1, 0, 4, "s_box", "684af5bc0acce85564bb0878242ed2ed"},
    {1, 0, 4, "s_row", "68cc08ed0abbd2bc642ef555244ae878"},
    {1, 0, 4, "m_col", "7a1e98bdacb6d1141a6944dd06eb2d3e"},
    {1, 0, 4, "k_sch", "58e151ab04a2a5557effb5416245080c"},
    {1, 0, 5, "start", "22ffc916a81474416496f19c64ae2532"},
    {1, 0, 5, "s_box", "9316dd47c2fa92834390a1de43e43f23"},
    {1, 0, 5, "s_row", "93faa123c2903f4743e4dd83431692de"},
    {1, 0, 5, "m_col", "aaa755b34cffe57cef6f98e1f01c13e6"},
    {1, 0, 5, "k_sch", "2ab54bb43a02f8f662e3a95d66410c08"},
    {1, 0, 6, "start", "80121e0776fd1d8a8d8c31bc965d1fee"},
    {1, 0, 6, "s_box", "cdc972c53854a47e5d64c765904cc028"},
    {1, 0, 6, "s_row", "cd54c7283864c0c55d4c727e90c9a465"},
    {1, 0, 6, "m_col", "921f748fd96e937d622d7725ba8ba50c"},
    {1, 0, 6, "k_sch", "f501857297448d7ebdf1c6ca87f33e3c"},
    {1, 0, 7, "start", "671ef1fd4e2a1e03dfdcb1ef3d789b30"},
    {1, 0, 7, "s_box", "8572a1542fe5727b9e86c8df27bc1404"},
    {1, 0, 7, "s_row", "85e5c8042f8614549ebca17b277272df"},
    {1, 0, 7, "m_col", "e913e7b18f507d4b227ef652758acbcc"},
    {1, 0, 7, "k_sch", "e510976183519b6934157c9ea351f1e0"},
    {1, 0, 8, "start", "0c0370d00c01e622166b8accd6db3a2c"},
    {1, 0, 8, "s_box", "fe7b5170fe7c8e93477f7e4bf6b98071"},
    {1, 0, 8, "s_row", "fe7c7e71fe7f807047b95193f67b8e4b"},
    {1, 0, 8, "m_col", "6cf5edf996eb0a069c4ef21cbfc25762"},
    {1, 0, 8, "k_sch", "1ea0372a995309167c439e77ff12051e"},
    {1, 0, 9, "start", "7255dad30fb80310e00d6c6b40d0527c"},
    {1, 0, 9, "s_box", "40fc5766766c7bcae1d7507f09700010"},
    {1, 0, 9, "s_row", "406c501076d70066e17057ca09fc7b7f"},
    {1, 0, 9, "m_col", "7478bcdce8a50b81d4327a9009188262"},
    {1, 0, 9, "k_sch", "dd7e0e887e2fff68608fc842f9dcc154"},
    {1, 0, 10, "start", "a906b254968af4e9b4bdb2d2f0c44336"},
    {1, 0, 10, "s_box", "d36f3720907ebf1e8d7a37b58c1c1a05"},
    {1, 0, 10, "s_row", "d37e3705907a1a208d1c371e8c6fbfb5"},
    {1, 0, 10, "m_col", "0d73cc2d8f6abe8b0cf2dd9bb83d422e"},
    {1, 0, 10, "k_sch", "859f5f237a8d5a3dc0c02952beefd63a"},
    {1, 0, 11, "start", "88ec930ef5e7e4b6cc32f4c906d29414"},
    {1, 0, 11, "s_box", "c4cedcabe694694e4b23bfdd6fb522fa"},
    {1, 0, 11, "s_row", "c494bffae62322ab4bb5dc4e6fce69dd"},
    {1, 0, 11, "m_col", "71d720933b6d677dc00b8f28238e0fb7"},
    {1, 0, 11, "k_sch", "de601e7827bcdf2ca223800fd8aeda32"},
    {1, 0, 12, "start", "afb73eeb1cd1b85162280f27fb20d585"},
    {1, 0, 12, "s_box", "79a9b2e99c3e6cd1aa3476cc0fb70397"},
    {1, 0, 12, "s_row", "793e76979c3403e9aab7b2d10fa96ccc"},
    {1, 0, 12, "k_sch", "a4970a331a78dc09c418c271e3a41d5d"},
    {1, 0, 12, "output", "dda97ca4864cdfe06eaf70a0ec0d7191"},
    {1, 1, 0, "iinput", "dda97ca4864cdfe06eaf70a0ec0d7191"},
    {1, 1, 0, "ik_sch", "a4970a331a78dc09c418c271e3a41d5d"},
    {1, 1, 1, "istart", "793e76979c3403e9aab7b2d10fa96ccc"},
    {1, 1, 1, "is_row", "79a9b2e99c3e6cd1aa3476cc0fb70397"},
    {1, 1, 1, "is_box", "afb73eeb1cd1b85162280f27fb20d585"},
    {1, 1, 1, "ik_sch", "de601e7827bcdf2ca223800fd8aeda32"},
    {1, 1, 1, "ik_add", "71d720933b6d677dc00b8f28238e0fb7"},
    {1, 1, 2, "istart", "c494bffae62322ab4bb5dc4e6fce69dd"},
    {1, 1, 2, "is_row", "c4cedcabe694694e4b23bfdd6fb522fa"},
    {1, 1, 2, "is_box", "88ec930ef5e7e4b6cc32f4c906d29414"},
    {1, 1, 2, "ik_sch", "859f5f237a8d5a3dc0c02952beefd63a"},
    {1, 1, 2, "ik_add", "0d73cc2d8f6abe8b0cf2dd9bb83d422e"},
    {1, 1, 3, "istart", "d37e3705907a1a208d1c371e8c6fbfb5"},
    {1, 1, 3, "is_row", "d36f3720907ebf1e8d7a37b58c1c1a05"},
    {1, 1, 3, "is_box", "a906b254968af4e9b4bdb2d2f0c44336"},
    {1, 1, 3, "ik_sch", "dd7e0e887e2fff68608fc842f9dcc154"},
    {1, 1, 3, "ik_add", "7478bcdce8a50b81d4327a9009188262"},
    {1, 1, 4, "istart", "406c501076d70066e17057ca09fc7b7f"},
    {1, 1, 4, "is_row", "40fc5766766c7bcae1d7507f09700010"},
    {1, 1, 4, "is_box", "7255dad30fb80310e00d6c6b40d0527c"},
    {1, 1, 4, "ik_sch", "1ea0372a995309167c439e77ff12051e"},
    {1, 1, 4, "ik_add", "6cf5edf996eb0a069c4ef21cbfc25762"},
    {1, 1, 5, "istart", "fe7c7e71fe7f807047b95193f67b8e4b"},
    {1, 1, 5, "is_row", "fe7b5170fe7c8e93477f7e4bf6b98071"},
    {1, 1, 5, "is_box", "0c0370d00c01e622166b8accd6db3a2c"},
    {1, 1, 5, "ik_sch", "e510976183519b6934157c9ea351f1e0"},
    {1, 1, 5, "ik_add", "e913e7b18f507d4b227ef652758acbcc"},
    {1, 1, 6, "istart", "85e5c8042f8614549ebca17b277272df"},
    {1, 1, 6, "is_row", "8572a1542fe5727b9e86c8df27bc1404"},
    {1, 1, 6, "is_box", "671ef1fd4e2a1e03dfdcb1ef3d789b30"},
    {1, 1, 6, "ik_sch", "f501857297448d7ebdf1c6ca87f33e3c"},
    {1, 1, 6, "ik_add", "921f748fd96e937d622d7725ba8ba50c"},
    {1, 1, 7, "istart", "cd54c7283864c0c55d4c727e90c9a465"},
    {1, 1, 7, "is_row", "cdc972c53854a47e5d64c765904cc028"},
    {1, 1, 7, "is_box", "80121e0776fd1d8a8d8c31bc965d1fee"},
    {1, 1, 7, "ik_sch", "2ab54bb43a02f8f662e3a95d66410c08"},
    {1, 1, 7, "ik_add", "aaa755b34cffe57cef6f98e1f01c13e6"},
    {1, 1, 8, "istart", "93faa123c2903f4743e4dd83431692de"},
    {1, 1, 8, "is_row", "9316dd47c2fa92834390a1de43e43f23"},
    {1, 1, 8, "is_box", "22ffc916a81474416496f19c64ae2532"},
    {1, 1, 8, "ik_sch", "58e151ab04a2a5557effb5416245080c"},
    {1, 1, 8, "ik_add", "7a1e98bdacb6d1141a6944dd06eb2d3e"},
    {1, 1, 9, "istart", "68cc08ed0abbd2bc642ef555244ae878"},
    {1, 1, 9, "is_row", "684af5bc0acce85564bb0878242ed2ed"},
    {1, 1, 9, "is_box", "f75c7778a327c8ed8cfebfc1a6c37f53"},
    {1, 1, 9, "ik_sch", "40f949b31cbabd4d48f043b810b7b342"},
    {1, 1, 9, "ik_add", "b7a53ecbbf9d75a0c40efc79b674cc11"},
    {1, 1, 10, "istart", "1fb5430ef0accf64aa370cde3d77792c"},
    {1, 1, 10, "is_row", "1f770c64f0b579deaaac432c3d37cf0e"},
    {1, 1, 10, "is_box", "cb02818c17d2af9c62aa64428bb25fd7"},
    {1, 1, 10, "ik_sch", "544afef55847f0fa4856e2e95c43f4fe"},
    {1, 1, 10, "ik_add", "9f487f794f955f662afc86abd7f1ab29"},
    {1, 1, 11, "istart", "84e1dd691a41d76f792d389783fbac70"},
    {1, 1, 11, "is_row", "84fb386f1ae1ac977941dd70832dd769"},
    {1, 1, 11, "is_box", "4f63760643e0aa85aff8c9d041fa0de4"},
    {1, 1, 11, "ik_sch", "10111213141516175846f2f95c43f4fe"},
    {1, 1, 11, "ik_add", "5f72641557f5bc92f7be3b291db9f91a"},
    {1, 1, 12, "istart", "6353e08c0960e104cd70b751bacad0e7"},
    {1, 1, 12, "is_row", "63cab7040953d051cd60e0e7ba70e18c"},
    {1, 1, 12, "is_box", "00102030405060708090a0b0c0d0e0f0"},
    {1, 1, 12, "ik_sch", "000102030405060708090a0b0c0d0e0f"},
    {1, 1, 12, "ioutput", "00112233445566778899aabbccddeeff"},
    {1, 2, 0, "iinput", "dda97ca4864cdfe06eaf70a0ec0d7191"},
    {1, 2, 0, "ik_sch", "a4970a331a78dc09c418c271e3a41d5d"},
    {1, 2, 1, "istart", "793e76979c3403e9aab7b2d10fa96ccc"},
    {1, 2, 1, "is_box", "afd10f851c28d5eb62203e51fbb7b827"},
    {1, 2, 1, "is_row", "afb73eeb1cd1b85162280f27fb20d585"},
    {1, 2, 1, "im_col", "122a02f7242ac8e20605afce51cc7264"},
    {1, 2, 1, "ik_sch", "d6bebd0dc209ea494db073803e021bb9"},
    {1, 2, 2, "istart", "c494bffae62322ab4bb5dc4e6fce69dd"},
    {1, 2, 2, "is_box", "88e7f414f532940eccd293b606ece4c9"},
    {1, 2, 2, "is_row", "88ec930ef5e7e4b6cc32f4c906d29414"},
    {1, 2, 2, "im_col", "5cc7aecce3c872194ae5ef8309a933c7"},
    {1, 2, 2, "ik_sch", "8fb999c973b26839c7f9d89d85c68c72"},
    {1, 2, 3, "istart", "d37e3705907a1a208d1c371e8c6fbfb5"},
    {1, 2, 3, "is_box", "a98ab23696bd4354b4c4b2e9f006f4d2"},
    {1, 2, 3, "is_row", "a906b254968af4e9b4bdb2d2f0c44336"},
    {1, 2, 3, "im_col", "b7113ed134e85489b20866b51d4b2c3b"},
    {1, 2, 3, "ik_sch", "f77d6ec1423f54ef5378317f14b75744"},
    {1, 2, 4, "istart", "406c501076d70066e17057ca09fc7b7f"},
    {1, 2, 4, "is_box", "72b86c7c0f0d52d3e0d0da104055036b"},
    {1, 2, 4, "is_row", "7255dad30fb80310e00d6c6b40d0527c"},
    {1, 2, 4, "im_col", "ef3b1be1b9b0e64bdcb79f1e0a707fbb"},
    {1, 2, 4, "ik_sch", "1147659047cf663b9b0ece8dfc0bf1f0"},
    {1, 2, 5, "istart", "fe7c7e71fe7f807047b95193f67b8e4b"},
    {1, 2, 5, "is_box", "0c018a2c0c6b3ad016db7022d603e6cc"},
    {1, 2, 5, "is_row", "0c0370d00c01e622166b8accd6db3a2c"},
    {1, 2, 5, "im_col", "592460b248832b2952e0b831923048f1"},
    {1, 2, 5, "ik_sch", "dcc1a8b667053f7dcc5c194ab5423a2e"},
    {1, 2, 6, "istart", "85e5c8042f8614549ebca17b277272df"},
    {1, 2, 6, "is_box", "672ab1304edc9bfddf78f1033d1e1eef"},
    {1, 2, 6, "is_row", "671ef1fd4e2a1e03dfdcb1ef3d789b30"},
    {1, 2, 6, "im_col", "0b8a7783417ae3a1f9492dc0c641a7ce"},
    {1, 2, 6, "ik_sch", "c6deb0ab791e2364a4055fbe568803ab"},
    {1, 2, 7, "istart", "cd54c7283864c0c55d4c727e90c9a465"},
    {1, 2, 7, "is_box", "80fd31ee768c1f078d5d1e8a96121dbc"},
    {1, 2, 7, "is_row", "80121e0776fd1d8a8d8c31bc965d1fee"},
    {1, 2, 7, "im_col", "4ee1ddf9301d6352c9ad769ef8d20515"},
    {1, 2, 7, "ik_sch", "dd1b7cdaf28d5c158a49ab1dbbc497cb"},
    {1, 2, 8, "istart", "93faa123c2903f4743e4dd83431692de"},
    {1, 2, 8, "is_box", "2214f132a896251664aec94164ff749c"},
    {1, 2, 8, "is_row", "22ffc916a81474416496f19c64ae2532"},
    {1, 2, 8, "im_col", "1008ffe53b36ee6af27b42549b8a7bb7"},
    {1, 2, 8, "ik_sch", "78c4f708318d3cd69655b701bfc093cf"},
    {1, 2, 9, "istart", "68cc08ed0abbd2bc642ef555244ae878"},
    {1, 2, 9, "is_box", "f727bf53a3fe7f788cc377eda65cc8c1"},
    {1, 2, 9, "is_row", "f75c7778a327c8ed8cfebfc1a6c37f53"},
    {1, 2, 9, "im_col", "7f69ac1ed939ebaac8ece3cb12e159e3"},
    {1, 2, 9, "ik_sch", "60dcef10299524ce62dbef152f9620cf"},
    {1, 2, 10, "istart", "1fb5430ef0accf64aa370cde3d77792c"},
    {1, 2, 10, "is_box", "cbd264d717aa5f8c62b2819c8b02af42"},
    {1, 2, 10, "is_row", "cb02818c17d2af9c62aa64428bb25fd7"},
    {1, 2, 10, "im_col", "cfaf16b2570c18b52e7fef50cab267ae"},
    {1, 2, 10, "ik_sch", "4b4ecbdb4d4dcfda5752d7c74949cbde"},
    {1, 2, 11, "istart", "84e1dd691a41d76f792d389783fbac70"},
    {1, 2, 11, "is_box", "4fe0c9e443f80d06affa76854163aad0"},
    {1, 2, 11, "is_row", "4f63760643e0aa85aff8c9d041fa0de4"},
    {1, 2, 11, "im_col", "794cf891177bfd1d8a327086f3831b39"},
    {1, 2, 11, "ik_sch", "1a1f181d1e1b1c194742c7d74949cbde"},
    {1, 2, 12, "istart", "6353e08c0960e104cd70b751bacad0e7"},
    {1, 2, 12, "is_box", "0050a0f04090e03080d02070c01060b0"},
    {1, 2, 12, "is_row", "00102030405060708090a0b0c0d0e0f0"},
    {1, 2, 12, "ik_sch", "000102030405060708090a0b0c0d0e0f"},
    {1, 2, 12, "ioutput", "00112233445566778899aabbccddeeff"},
    {2, 0, 0, "input", "00112233445566778899aabbccddeeff"},
    {2, 0, 0, "k_sch", "000102030405060708090a0b0c0d0e0f"},
    {2, 0, 1, "start", "00102030405060708090a0b0c0d0e0f0"},
    {2, 0, 1, "s_box", "63cab7040953d051cd60e0e7ba70e18c"},
    {2, 0, 1, "s_row", "6353e08c0960e104cd70b751bacad0e7"},
    {2, 0, 1, "m_col", "5f72641557f5bc92f7be3b291db9f91a"},
    {2, 0, 1, "k_sch", "101112131415161718191a1b1c1d1e1f"},
    {2, 0, 2, "start", "4f63760643e0aa85efa7213201a4e705"},
    {2, 0, 2, "s_box", "84fb386f1ae1ac97df5cfd237c49946b"},
    {2, 0, 2, "s_row", "84e1fd6b1a5c946fdf4938977cfbac23"},
    {2, 0, 2, "m_col", "bd2a395d2b6ac438d192443e615da195"},
    {2, 0, 2, "k_sch", "a573c29fa176c498a97fce93a572c09c"},
    {2, 0, 3, "start", "1859fbc28a1c00a078ed8aadc42f6109"},
    {2, 0, 3, "s_box", "adcb0f257e9c63e0bc557e951c15ef01"},
    {2, 0, 3, "s_row", "ad9c7e017e55ef25bc150fe01ccb6395"},
    {2, 0, 3, "m_col", "810dce0cc9db8172b3678c1e88a1b5bd"},
    {2, 0, 3, "k_sch", "1651a8cd0244beda1a5da4c10640bade"},
    {2, 0, 4, "start", "975c66c1cb9f3fa8a93a28df8ee10f63"},
    {2, 0, 4, "s_box", "884a33781fdb75c2d380349e19f876fb"},
    {2, 0, 4, "s_row", "88db34fb1f807678d3f833c2194a759e"},
    {2, 0, 4, "m_col", "b2822d81abe6fb275faf103a078c0033"},
    {2, 0, 4, "k_sch", "ae87dff00ff11b68a68ed5fb03fc1567"},
    {2, 0, 5, "start", "1c05f271a417e04ff921c5c104701554"},
    {2, 0, 5, "s_box", "9c6b89a349f0e18499fda678f2515920"},
    {2, 0, 5, "s_row", "9cf0a62049fd59a399518984f26be178"},
    {2, 0, 5, "m_col", "aeb65ba974e0f822d73f567bdb64c877"},
    {2, 0, 5, "k_sch", "6de1f1486fa54f9275f8eb5373b8518d"},
    {2, 0, 6, "start", "c357aae11b45b7b0a2c7bd28a8dc99fa"},
    {2, 0, 6, "s_box", "2e5bacf8af6ea9e73ac67a34c286ee2d"},
    {2, 0, 6, "s_row", "2e6e7a2dafc6eef83a86ace7c25ba934"},
    {2, 0, 6, "m_col", "b951c33c02e9bd29ae25cdb1efa08cc7"},
    {2, 0, 6, "k_sch", "c656827fc9a799176f294cec6cd5598b"},
    {2, 0, 7, "start", "7f074143cb4e243ec10c815d8375d54c"},
    {2, 0, 7, "s_box", "d2c5831a1f2f36b278fe0c4cec9d0329"},
    {2, 0, 7, "s_row", "d22f0c291ffe031a789d83b2ecc5364c"},
    {2, 0, 7, "m_col", "ebb19e1c3ee7c9e87d7535e9ed6b9144"},
    {2, 0, 7, "k_sch", "3de23a75524775e727bf9eb45407cf39"},
    {2, 0, 8, "start", "d653a4696ca0bc0f5acaab5db96c5e7d"},
    {2, 0, 8, "s_box", "f6ed49f950e06576be74624c565058ff"},
    {2, 0, 8, "s_row", "f6e062ff507458f9be50497656ed654c"},
    {2, 0, 8, "m_col", "5174c8669da98435a8b3e62ca974a5ea"},
    {2, 0, 8, "k_sch", "0bdc905fc27b0948ad5245a4c1871c2f"},
    {2, 0, 9, "start", "5aa858395fd28d7d05e1a38868f3b9c5"},
    {2, 0, 9, "s_box", "bec26a12cfb55dff6bf80ac4450d56a6"},
    {2, 0, 9, "s_row", "beb50aa6cff856126b0d6aff45c25dc4"},
    {2, 0, 9, "m_col", "0f77ee31d2ccadc05430a83f4ef96ac3"},
    {2, 0, 9, "k_sch", "45f5a66017b2d387300d4d33640a820a"},
    {2, 0, 10, "start", "4a824851c57e7e47643de50c2af3e8c9"},
    {2, 0, 10, "s_box", "d61352d1a6f3f3a04327d9fee50d9bdd"},
    {2, 0, 10, "s_row", "d6f3d9dda6279bd1430d52a0e513f3fe"},
    {2, 0, 10, "m_col", "bd86f0ea748fc4f4630f11c1e9331233"},
    {2, 0, 10, "k_sch", "7ccff71cbeb4fe5413e6bbf0d261a7df"},
    {2, 0, 11, "start", "c14907f6ca3b3aa070e9aa313b52b5ec"},
    {2, 0, 11, "s_box", "783bc54274e280e0511eacc7e200d5ce"},
    {2, 0, 11, "s_row", "78e2acce741ed5425100c5e0e23b80c7"},
    {2, 0, 11, "m_col", "af8690415d6e1dd387e5fbedd5c89013"},
    {2, 0, 11, "k_sch", "f01afafee7a82979d7a5644ab3afe640"},
    {2, 0, 12, "start", "5f9c6abfbac634aa50409fa766677653"},
    {2, 0, 12, "s_box", "cfde0208f4b418ac5309db5c338538ed"},
    {2, 0, 12, "s_row", "cfb4dbedf4093808538502ac33de185c"},
    {2, 0, 12, "m_col", "7427fae4d8a695269ce83d315be0392b"},
    {2, 0, 12, "k_sch", "2541fe719bf500258813bbd55a721c0a"},
    {2, 0, 13, "start", "516604954353950314fb86e401922521"},
    {2, 0, 13, "s_box", "d133f22a1aed2a7bfa0f44697c4f3ffd"},
    {2, 0, 13, "s_row", "d1ed44fd1a0f3f2afa4ff27b7c332a69"},
    {2, 0, 13, "m_col", "2c21a820306f154ab712c75eee0da04f"},
    {2, 0, 13, "k_sch", "4e5a6699a9f24fe07e572baacdf8cdea"},
    {2, 0, 14, "start", "627bceb9999d5aaac945ecf423f56da5"},
    {2, 0, 14, "s_box", "aa218b56ee5ebeacdd6ecebf26e63c06"},
    {2, 0, 14, "s_row", "aa5ece06ee6e3c56dde68bac2621bebf"},
    {2, 0, 14, "k_sch", "24fc79ccbf0979e9371ac23c6d68de36"},
    {2, 0, 14, "output", "8ea2b7ca516745bfeafc49904b496089"},
    {2, 1, 0, "iinput", "8ea2b7ca516745bfeafc49904b496089"},
    {2, 1, 0, "ik_sch", "24fc79ccbf0979e9371ac23c6d68de36"},
    {2, 1, 1, "istart", "aa5ece06ee6e3c56dde68bac2621bebf"},
    {2, 1, 1, "is_row", "aa218b56ee5ebeacdd6ecebf26e63c06"},
    {2, 1, 1, "is_box", "627bceb9999d5aaac945ecf423f56da5"},
    {2, 1, 1, "ik_sch", "4e5a6699a9f24fe07e572baacdf8cdea"},
    {2, 1, 1, "ik_add", "2c21a820306f154ab712c75eee0da04f"},
    {2, 1, 2, "istart", "d1ed44fd1a0f3f2afa4ff27b7c332a69"},
    {2, 1, 2, "is_row", "d133f22a1aed2a7bfa0f44697c4f3ffd"},
    {2, 1, 2, "is_box", "516604954353950314fb86e401922521"},
    {2, 1, 2, "ik_sch", "2541fe719bf500258813bbd55a721c0a"},
    {2, 1, 2, "ik_add", "7427fae4d8a695269ce83d315be0392b"},
    {2, 1, 3, "istart", "cfb4dbedf4093808538502ac33de185c"},
    {2, 1, 3, "is_row", "cfde0208f4b418ac5309db5c338538ed"},
    {2, 1, 3, "is_box", "5f9c6abfbac634aa50409fa766677653"},
    {2, 1, 3, "ik_sch", "f01afafee7a82979d7a5644ab3afe640"},
    {2, 1, 3, "ik_add", "af8690415d6e1dd387e5fbedd5c89013"},
    {2, 1, 4, "istart", "78e2acce741ed5425100c5e0e23b80c7"},
    {2, 1, 4, "is_row", "783bc54274e280e0511eacc7e200d5ce"},
    {2, 1, 4, "is_box", "c14907f6ca3b3aa070e9aa313b52b5ec"},
    {2, 1, 4, "ik_sch", "7ccff71cbeb4fe5413e6bbf0d261a7df"},
    {2, 1, 4, "ik_add", "bd86f0ea748fc4f4630f11c1e9331233"},
    {2, 1, 5, "istart", "d6f3d9dda6279bd1430d52a0e513f3fe"},
    {2, 1, 5, "is_row", "d61352d1a6f3f3a04327d9fee50d9bdd"},
    {2, 1, 5, "is_box", "4a824851c57e7e47643de50c2af3e8c9"},
    {2, 1, 5, "ik_sch", "45f5a66017b2d387300d4d33640a820a"},
    {2, 1, 5, "ik_add", "0f77ee31d2ccadc05430a83f4ef96ac3"},
    {2, 1, 6, "istart", "beb50aa6cff856126b0d6aff45c25dc4"},
    {2, 1, 6, "is_row", "bec26a12cfb55dff6bf80ac4450d56a6"},
    {2, 1, 6, "is_box", "5aa858395fd28d7d05e1a38868f3b9c5"},
    {2, 1, 6, "ik_sch", "0bdc905fc27b0948ad5245a4c1871c2f"},
    {2, 1, 6, "ik_add", "5174c8669da98435a8b3e62ca974a5ea"},
    {2, 1, 7, "istart", "f6e062ff507458f9be50497656ed654c"},
    {2, 1, 7, "is_row", "f6ed49f950e06576be74624c565058ff"},
    {2, 1, 7, "is_box", "d653a4696ca0bc0f5acaab5db96c5e7d"},
    {2, 1, 7, "ik_sch", "3de23a75524775e727bf9eb45407cf39"},
    {2, 1, 7, "ik_add", "ebb19e1c3ee7c9e87d7535e9ed6b9144"},
    {2, 1, 8, "istart", "d22f0c291ffe031a789d83b2ecc5364c"},
    {2, 1, 8, "is_row", "d2c5831a1f2f36b278fe0c4cec9d0329"},
    {2, 1, 8, "is_box", "7f074143cb4e243ec10c815d8375d54c"},
    {2, 1, 8, "ik_sch", "c656827fc9a799176f294cec6cd5598b"},
    {2, 1, 8, "ik_add", "b951c33c02e9bd29ae25cdb1efa08cc7"},
    {2, 1, 9, "istart", "2e6e7a2dafc6eef83a86ace7c25ba934"},
    {2, 1, 9, "is_row", "2e5bacf8af6ea9e73ac67a34c286ee2d"},
    {2, 1, 9, "is_box", "c357aae11b45b7b0a2c7bd28a8dc99fa"},
    {2, 1, 9, "ik_sch", "6de1f1486fa54f9275f8eb5373b8518d"},
    {2, 1, 9, "ik_add", "aeb65ba974e0f822d73f567bdb64c877"},
    {2, 1, 10, "istart", "9cf0a62049fd59a399518984f26be178"},
    {2, 1, 10, "is_row", "9c6b89a349f0e18499fda678f2515920"},
    {2, 1, 10, "is_box", "1c05f271a417e04ff921c5c104701554"},
    {2, 1, 10, "ik_sch", "ae87dff00ff11b68a68ed5fb03fc1567"},
    {2, 1, 10, "ik_add", "b2822d81abe6fb275faf103a078c0033"},
    {2, 1, 11, "istart", "88db34fb1f807678d3f833c2194a759e"},
    {2, 1, 11, "is_row", "884a33781fdb75c2d380349e19f876fb"},
    {2, 1, 11, "is_box", "975c66c1cb9f3fa8a93a28df8ee10f63"},
    {2, 1, 11, "ik_sch", "1651a8cd0244beda1a5da4c10640bade"},
    {2, 1, 11, "ik_add", "810dce0cc9db8172b3678c1e88a1b5bd"},
    {2, 1, 12, "istart", "ad9c7e017e55ef25bc150fe01ccb6395"},
    {2, 1, 12, "is_row", "adcb0f257e9c63e0bc557e951c15ef01"},
    {2, 1, 12, "is_box", "1859fbc28a1c00a078ed8aadc42f6109"},
    {2, 1, 12, "ik_sch", "a573c29fa176c498a97fce93a572c09c"},
    {2, 1, 12, "ik_add", "bd2a395d2b6ac438d192443e615da195"},
    {2, 1, 13, "istart", "84e1fd6b1a5c946fdf4938977cfbac23"},
    {2, 1, 13, "is_row", "84fb386f1ae1ac97df5cfd237c49946b"},
    {2, 1, 13, "is_box", "4f63760643e0aa85efa7213201a4e705"},
    {2, 1, 13, "ik_sch", "101112131415161718191a1b1c1d1e1f"},
    {2, 1, 13, "ik_add", "5f72641557f5bc92f7be3b291db9f91a"},
    {2, 1, 14, "istart", "6353e08c0960e104cd70b751bacad0e7"},
    {2, 1, 14, "is_row", "63cab7040953d051cd60e0e7ba70e18c"},
    {2, 1, 14, "is_box", "00102030405060708090a0b0c0d0e0f0"},
    {2, 1, 14, "ik_sch", "000102030405060708090a0b0c0d0e0f"},
    {2, 1, 14, "ioutput", "00112233445566778899aabbccddeeff"},
    {2, 2, 0, "iinput", "8ea2b7ca516745bfeafc49904b496089"},
    {2, 2, 0, "ik_sch", "24fc79ccbf0979e9371ac23c6d68de36"},
    {2, 2, 1, "istart", "aa5ece06ee6e3c56dde68bac2621bebf"},
    {2, 2, 1, "is_box", "629deca599456db9c9f5ceaa237b5af4"},
    {2, 2, 1, "is_row", "627bceb9999d5aaac945ecf423f56da5"},
    {2, 2, 1, "im_col", "e51c9502a5c1950506a61024596b2b07"},
    {2, 2, 1, "ik_sch", "34f1d1ffbfceaa2ffce9e25f2558016e"},
    {2, 2, 2, "istart", "d1ed44fd1a0f3f2afa4ff27b7c332a69"},
    {2, 2, 2, "is_box", "5153862143fb259514920403016695e4"},
    {2, 2, 2, "is_row", "516604954353950314fb86e401922521"},
    {2, 2, 2, "im_col", "91a29306cc450d0226f4b5eaef5efed8"},
    {2, 2, 2, "ik_sch", "5e1648eb384c350a7571b746dc80e684"},
    {2, 2, 3, "istart", "cfb4dbedf4093808538502ac33de185c"},
    {2, 2, 3, "is_box", "5fc69f53ba4076bf50676aaa669c34a7"},
    {2, 2, 3, "is_row", "5f9c6abfbac634aa50409fa766677653"},
    {2, 2, 3, "im_col", "b041a94eff21ae9212278d903b8a63f6"},
    {2, 2, 3, "ik_sch", "c8a305808b3f7bd043274870d9b1e331"},
    {2, 2, 4, "istart", "78e2acce741ed5425100c5e0e23b80c7"},
    {2, 2, 4, "is_box", "c13baaeccae9b5f6705207a03b493a31"},
    {2, 2, 4, "is_row", "c14907f6ca3b3aa070e9aa313b52b5ec"},
    {2, 2, 4, "im_col", "638357cec07de6300e30d0ec4ce2a23c"},
    {2, 2, 4, "ik_sch", "b5708e13665a7de14d3d824ca9f151c2"},
    {2, 2, 5, "istart", "d6f3d9dda6279bd1430d52a0e513f3fe"},
    {2, 2, 5, "is_box", "4a7ee5c9c53de85164f348472a827e0c"},
    {2, 2, 5, "is_row", "4a824851c57e7e47643de50c2af3e8c9"},
    {2, 2, 5, "im_col", "ca6f71058c642842a315595fdf54f685"},
    {2, 2, 5, "ik_sch", "74da7ba3439c7e50c81833a09a96ab41"},
    {2, 2, 6, "istart", "beb50aa6cff856126b0d6aff45c25dc4"},
    {2, 2, 6, "is_box", "5ad2a3c55fe1b93905f3587d68a88d88"},
    {2, 2, 6, "is_row", "5aa858395fd28d7d05e1a38868f3b9c5"},
    {2, 2, 6, "im_col", "ca46f5ea835eab0b9537b6dbb221b6c2"},
    {2, 2, 6, "ik_sch", "3ca69715d32af3f22b67ffade4ccd38e"},
    {2, 2, 7, "istart", "f6e062ff507458f9be50497656ed654c"},
    {2, 2, 7, "is_box", "d6a0ab7d6cca5e695a6ca40fb953bc5d"},
    {2, 2, 7, "is_row", "d653a4696ca0bc0f5acaab5db96c5e7d"},
    {2, 2, 7, "im_col", "2a70c8da28b806e9f319ce42be4baead"},
    {2, 2, 7, "ik_sch", "f85fc4f3374605f38b844df0528e98e1"},
    {2, 2, 8, "istart", "d22f0c291ffe031a789d83b2ecc5364c"},
    {2, 2, 8, "is_box", "7f4e814ccb0cd543c175413e8307245d"},
    {2, 2, 8, "is_row", "7f074143cb4e243ec10c815d8375d54c"},
    {2, 2, 8, "im_col", "f0073ab7404a8a1fc2cba0b80df08517"},
    {2, 2, 8, "ik_sch", "de69409aef8c64e7f84d0c5fcfab2c23"},
    {2, 2, 9, "istart", "2e6e7a2dafc6eef83a86ace7c25ba934"},
    {2, 2, 9, "is_box", "c345bdfa1bc799e1a2dcaab0a857b728"},
    {2, 2, 9, "is_row", "c357aae11b45b7b0a2c7bd28a8dc99fa"},
    {2, 2, 9, "im_col", "3225fe3686e498a32593c1872b613469"},
    {2, 2, 9, "ik_sch", "aed55816cf19c100bcc24803d90ad511"},
    {2, 2, 10, "istart", "9cf0a62049fd59a399518984f26be178"},
    {2, 2, 10, "is_box", "1c17c554a4211571f970f24f0405e0c1"},
    {2, 2, 10, "is_row", "1c05f271a417e04ff921c5c104701554"},
    {2, 2, 10, "im_col", "9d1d5c462e655205c4395b7a2eac55e2"},
    {2, 2, 10, "ik_sch", "15c668bd31e5247d17c168b837e6207c"},
    {2, 2, 11, "istart", "88db34fb1f807678d3f833c2194a759e"},
    {2, 2, 11, "is_box", "979f2863cb3a0fc1a9e166a88e5c3fdf"},
    {2, 2, 11, "is_row", "975c66c1cb9f3fa8a93a28df8ee10f63"},
    {2, 2, 11, "im_col", "d24bfb0e1f997633cfce86e37903fe87"},
    {2, 2, 11, "ik_sch", "7fd7850f61cc991673db890365c89d12"},
    {2, 2, 12, "istart", "ad9c7e017e55ef25bc150fe01ccb6395"},
    {2, 2, 12, "is_box", "181c8a098aed61c2782ffba0c45900ad"},
    {2, 2, 12, "is_row", "1859fbc28a1c00a078ed8aadc42f6109"},
    {2, 2, 12, "im_col", "aec9bda23e7fd8aff96d74525cdce4e7"},
    {2, 2, 12, "ik_sch", "2a2840c924234cc026244cc5202748c4"},
    {2, 2, 13, "istart", "84e1fd6b1a5c946fdf4938977cfbac23"},
    {2, 2, 13, "is_box", "4fe0210543a7e706efa476850163aa32"},
    {2, 2, 13, "is_row", "4f63760643e0aa85efa7213201a4e705"},
    {2, 2, 13, "im_col", "794cf891177bfd1ddf67a744acd9c4f6"},
    {2, 2, 13, "ik_sch", "1a1f181d1e1b1c191217101516131411"},
    {2, 2, 14, "istart", "6353e08c0960e104cd70b751bacad0e7"},
    {2, 2, 14, "is_box", "0050a0f04090e03080d02070c01060b0"},
    {2, 2, 14, "is_row", "00102030405060708090a0b0c0d0e0f0"},
    {2, 2, 14, "ik_sch", "000102030405060708090a0b0c0d0e0f"},
    {2, 2, 14, "ioutput", "00112233445566778899aabbccddeeff"},
};

static int hexbytes(const char *s, u8 *out) {
    int n = 0;
    for (; s[0] && s[1]; s += 2) {
        int v = 0;
        for (int i = 0; i < 2; i++) { char c = s[i]; v = v * 16 + (c <= '9' ? c - '0' : c >= 'a' ? c - 'a' + 10 : c - 'A' + 10); }
        out[n++] = (u8)v;
    }
    return n;
}
static int streq(const char *a, const char *b) { while (*a && *a == *b) { a++; b++; } return *a == *b; }

// ---- AES / Rijndael with a 128-bit block (FIPS 197, section 5; GF(2^8) with x^8 + x^4 + x^3 + x + 1) ----
#ifndef AES_AFFINE_C
#define AES_AFFINE_C 0x63          // constant of the S-box affine transformation
#endif
static u8 xtime(u8 a) { return (u8)((a << 1) ^ ((a & 0x80) ? 0x1b : 0)); }
static u8 gmul(u8 a, u8 b) { u8 p = 0; while (b) { if (b & 1) p ^= a; a = xtime(a); b >>= 1; } return p; }
static u8 SBOX[256], INV[256];
static u8 rotl8(u8 v, int n) { return (u8)((v << n) | (v >> (8 - n))); }
static void make_sbox(void) {
    for (int x = 0; x < 256; x++) {
        // multiplicative inverse as x^254 (0 maps to 0), then the affine transformation
        u8 r = 1, b = (u8)x; int e = 254;
        while (e) { if (e & 1) r = gmul(r, b); b = gmul(b, b); e >>= 1; }
        u8 s = (u8)(r ^ rotl8(r, 1) ^ rotl8(r, 2) ^ rotl8(r, 3) ^ rotl8(r, 4) ^ AES_AFFINE_C);
        SBOX[x] = s; INV[s] = (u8)x;
    }
}
static u8 RK[15][16];
static void key_expand(const u8 *key, int nk, int nr) {
    u8 w[60][4]; u8 rcon = 1;
    for (int i = 0; i < nk; i++) for (int j = 0; j < 4; j++) w[i][j] = key[4 * i + j];
    for (int i = nk; i < 4 * (nr + 1); i++) {
        u8 t[4]; for (int j = 0; j < 4; j++) t[j] = w[i - 1][j];
        if (i % nk == 0) {
            u8 t0 = t[0]; t[0] = (u8)(SBOX[t[1]] ^ rcon); t[1] = SBOX[t[2]]; t[2] = SBOX[t[3]]; t[3] = SBOX[t0];
            rcon = xtime(rcon);
        } else if (nk > 6 && i % nk == 4) { for (int j = 0; j < 4; j++) t[j] = SBOX[t[j]]; }
        for (int j = 0; j < 4; j++) w[i][j] = (u8)(w[i - nk][j] ^ t[j]);
    }
    for (int r = 0; r <= nr; r++) for (int c = 0; c < 4; c++) for (int j = 0; j < 4; j++) RK[r][4 * c + j] = w[4 * r + c][j];
}
// state byte i is row i%4, column i/4 (the input order of FIPS 197 section 3.4)
static void sub_bytes(u8 *s, const u8 *box) { for (int i = 0; i < 16; i++) s[i] = box[s[i]]; }
static void shift_rows(u8 *s) { u8 t[16]; for (int r = 0; r < 4; r++) for (int c = 0; c < 4; c++) t[r + 4 * c] = s[r + 4 * ((c + r) & 3)]; for (int i = 0; i < 16; i++) s[i] = t[i]; }
static void inv_shift_rows(u8 *s) { u8 t[16]; for (int r = 0; r < 4; r++) for (int c = 0; c < 4; c++) t[r + 4 * ((c + r) & 3)] = s[r + 4 * c]; for (int i = 0; i < 16; i++) s[i] = t[i]; }
static void mix(u8 *s, u8 a0, u8 a1, u8 a2, u8 a3) {   // multiply each column by the circulant matrix (a0 a1 a2 a3)
    for (int c = 0; c < 4; c++) {
        u8 *p = s + 4 * c, x0 = p[0], x1 = p[1], x2 = p[2], x3 = p[3];
        p[0] = (u8)(gmul(x0, a0) ^ gmul(x1, a1) ^ gmul(x2, a2) ^ gmul(x3, a3));
        p[1] = (u8)(gmul(x0, a3) ^ gmul(x1, a0) ^ gmul(x2, a1) ^ gmul(x3, a2));
        p[2] = (u8)(gmul(x0, a2) ^ gmul(x1, a3) ^ gmul(x2, a0) ^ gmul(x3, a1));
        p[3] = (u8)(gmul(x0, a1) ^ gmul(x1, a2) ^ gmul(x2, a3) ^ gmul(x3, a0));
    }
}
static void mix_columns(u8 *s) { mix(s, 2, 3, 1, 1); }
static void inv_mix_columns(u8 *s) { mix(s, 14, 11, 13, 9); }
static void add_key(u8 *s, const u8 *k) { for (int i = 0; i < 16; i++) s[i] ^= k[i]; }

// Every intermediate value the program computes is looked up in the published table of FIPS 197 Appendix C and compared.
static int hit[sizeof REC / sizeof REC[0]];
static int total_checked, total_bad;
static void chk(int ks, int sec, int round, const char *tag, const u8 *state) {
    for (unsigned i = 0; i < sizeof REC / sizeof REC[0]; i++) {
        if (REC[i].ks != ks || REC[i].sec != sec || REC[i].round != round || !streq(REC[i].tag, tag)) continue;
        u8 want[16]; hexbytes(REC[i].hex, want);
        int ok = 1; for (int j = 0; j < 16; j++) if (state[j] != want[j]) ok = 0;
        hit[i]++; total_checked++;
        if (!ok) { total_bad++; pstr("FAIL C."); pdec(ks + 1); pstr(" sec "); pdec(sec); pstr(" round "); pdec(round); pstr(" "); pstr(tag); pstr(": "); phex_bytes(state, 16); pstr(" expected "); phex_bytes(want, 16); pstr("\n"); }
        return;
    }
}
int paper_main(void) {
    make_sbox();
    static const char *NAMES[3] = {"AES-128", "AES-192", "AES-256"};
    for (int ks = 0; ks < 3; ks++) {
        int nk = 4 + 2 * ks, nr = nk + 6;
        u8 key[32], pt[16], s[16], ct[16];
        hexbytes(KEYS[ks], key); hexbytes(PLAIN, pt);
        key_expand(key, nk, nr);
        int c0 = total_checked, b0 = total_bad;
        // Cipher (FIPS 197 section 5.1)
        for (int i = 0; i < 16; i++) s[i] = pt[i];
        chk(ks, 0, 0, "input", s); chk(ks, 0, 0, "k_sch", RK[0]); add_key(s, RK[0]);
        for (int r = 1; r <= nr; r++) {
            chk(ks, 0, r, "start", s);
            sub_bytes(s, SBOX); chk(ks, 0, r, "s_box", s);
            shift_rows(s); chk(ks, 0, r, "s_row", s);
            if (r < nr) { mix_columns(s); chk(ks, 0, r, "m_col", s); }
            chk(ks, 0, r, "k_sch", RK[r]); add_key(s, RK[r]);
        }
        chk(ks, 0, nr, "output", s);
        for (int i = 0; i < 16; i++) ct[i] = s[i];
        // Inverse Cipher (section 5.3)
        chk(ks, 1, 0, "iinput", s); chk(ks, 1, 0, "ik_sch", RK[nr]); add_key(s, RK[nr]);
        for (int r = 1; r <= nr; r++) {
            chk(ks, 1, r, "istart", s);
            inv_shift_rows(s); chk(ks, 1, r, "is_row", s);
            sub_bytes(s, INV); chk(ks, 1, r, "is_box", s);
            chk(ks, 1, r, "ik_sch", RK[nr - r]); add_key(s, RK[nr - r]);
            if (r < nr) { chk(ks, 1, r, "ik_add", s); inv_mix_columns(s); }
        }
        chk(ks, 1, nr, "ioutput", s);
        int rt = 1; for (int i = 0; i < 16; i++) if (s[i] != pt[i]) rt = 0;
        // Equivalent Inverse Cipher (section 5.3.5): InvMixColumns applied to round keys 1..Nr-1
        for (int i = 0; i < 16; i++) s[i] = ct[i];
        chk(ks, 2, 0, "iinput", s); chk(ks, 2, 0, "ik_sch", RK[nr]); add_key(s, RK[nr]);
        for (int r = 1; r <= nr; r++) {
            u8 dk[16]; for (int i = 0; i < 16; i++) dk[i] = RK[nr - r][i];
            chk(ks, 2, r, "istart", s);
            sub_bytes(s, INV); chk(ks, 2, r, "is_box", s);
            inv_shift_rows(s); chk(ks, 2, r, "is_row", s);
            if (r < nr) { inv_mix_columns(s); chk(ks, 2, r, "im_col", s); inv_mix_columns(dk); }
            chk(ks, 2, r, "ik_sch", dk); add_key(s, dk);
        }
        chk(ks, 2, nr, "ioutput", s);
        for (int i = 0; i < 16; i++) if (s[i] != pt[i]) rt = 0;
        pstr(NAMES[ks]); pstr(": ciphertext "); phex_bytes(ct, 16);
        pstr(", "); pdec(total_checked - c0); pstr(" Appendix C values checked, "); pdec(total_bad - b0); pstr(" mismatches");
        pstr(rt ? ", both inverses return the plaintext\n" : ", INVERSE FAILED\n");
        if (!rt) total_bad++;
    }
    int unchecked = 0; for (unsigned i = 0; i < sizeof REC / sizeof REC[0]; i++) if (hit[i] != 1) unchecked++;
    pstr("published values compared: "); pdec(total_checked); pstr(" of "); pdec((i64)(sizeof REC / sizeof REC[0]));
    pstr(", mismatches "); pdec(total_bad); pstr(", values not compared exactly once "); pdec(unchecked); pstr("\n");
    int bad = total_bad || unchecked;
    pstr(bad ? "FAIL\n" : "PASS\n");
    return bad ? 1 : 0;
}
