# Sources

Every file the `classics` pack (repository `bentwire-classics`) is generated from, every file
the real-data tests read, and every Microsoft binary that was analysed to write
`docs/spec.md`, `docs/character-format.md` and `docs/research/`. Nothing listed here is part
of this repository.

Each file is identified by product, version, original file name, size and SHA-256. To run
the real-data tests or regenerate the pack, set `BENTWIRE_SOURCES` to one or more
directories (colon-separated) with your own copies of these files, in any layout: the tests
and `sources.sh` in `bentwire-classics` find each file by its file name and check its
SHA-256. The tests read the tables below (first column the file name, SHA-256 in
backticks), so keep that layout. Sizes and SHA-256 values were computed from the files on
2026-11-11.

Version column: for ACS files the file version field, for ACF and Agent 1.5 files the ACF
version, for ACT files the resource pack version (`docs/research/act-format.md` section 1:
Office 97 actors use 2.2, Microsoft Bob actors 1.0), for binaries the FileVersion.

## 1. Character sources

### 1.1 Agent 2.0 characters (ACS)

All have version 0x20001 and animation set 2.0 (`docs/research/acs-format.md` section 6).

| File | Product | Version | Bytes | SHA-256 |
|---|---|---|---|---|
| `CLIPPIT.ACS` | Microsoft Agent 2.0 character | ACS 0x20001 | 2,904,417 | `0ed1ab3ee8f349d0c2b5446029049be1d64c2294b2b72343ddd5e69e62e08e5f` |
| `DOT.ACS` | Microsoft Agent 2.0 character | ACS 0x20001 | 555,163 | `278359f2e866e43142f2bc6205544e97a241292d57ff7cebd63376358a0c0902` |
| `F1.ACS` | Microsoft Agent 2.0 character | ACS 0x20001 | 2,554,070 | `a27cac0b65264634c20b48dab2e8342d37d5351493620dd073e54969c611d800` |
| `LOGO.ACS` | Microsoft Agent 2.0 character | ACS 0x20001 | 1,030,546 | `9abeab7b8b62599b6afdd13d44d79b5566ceb523711c092e72eb185fb419426c` |
| `MNATURE.ACS` | Microsoft Agent 2.0 character | ACS 0x20001 | 1,530,968 | `7f305422bc0721b275a2b88fb0679d8e363fe7b0c55ffde8bbe9aa439df44db8` |
| `OFFCAT.ACS` | Microsoft Agent 2.0 character | ACS 0x20001 | 2,071,708 | `ee578e00ee2ed8f64f0efbafd83cc80a9ac715ce5c1f08db52d5f693f8288322` |
| `ROCKY.ACS` | Microsoft Agent 2.0 character | ACS 0x20001 | 3,006,178 | `952ba4531bf748dacdc7314e7b934889fb06a01687f00f7b0e10850112788eea` |
| `courtney.acs` | Microsoft Agent 2.0 character | ACS 0x20001 | 816,535 | `daa0c43f23a91674f62600c64acfd4c29636c86db4248ab01644e3d04664d92d` |
| `earl.acs` | Microsoft Agent 2.0 character | ACS 0x20001 | 1,472,718 | `7698d85971d3b19ed424b841624d87a740d2cd7062becf1e71d96c16c2999a90` |
| `merlin.acs` | Microsoft Agent 2.0 character | ACS 0x20001 | 2,180,663 | `a88151f313bea60002c5e5cb8b4d7dc5e2d919ba8fe78db1a4d05279a9ad58d7` |
| `qmark.acs` | Microsoft Agent 2.0 character | ACS 0x20001 | 1,174,050 | `08a13b0f04e97da4a6d8be0ddbec1e6f1b1244398109cfe3fcd518eb7332d163` |
| `rover.acs` | Microsoft Agent 2.0 character | ACS 0x20001 | 1,861,820 | `1b2cd6df82cf583d6d8817c19f5154b221d33b00650d41359a917080b64327d2` |

### 1.2 Agent 2.0 web characters (ACF + ACA)

Microsoft's freely distributed Agent 2.0 web characters, `docs/research/acf-aaf-format.md`
section 3. Each ACF sits in a folder with its animation files (`<File>.aca`, 86 files,
3,419,499 bytes in total, all version 0x20001, listed by name in `acf-aaf-format.md`
section 3). None of the four is in the pack: 216 of their animations (Genie 64, Merlin 28,
Peedy 72, Robby 52) did not survive.

| File | Product | Version | Bytes | SHA-256 |
|---|---|---|---|---|
| `genie.acf` | Microsoft Agent 2.0 web character | ACF 0x20001 | 8,325 | `3d94ea569c8d8bbb99d7cdf500d32db44b70e686c912edb39617eaabab5bbccb` |
| `merlin.acf` | Microsoft Agent 2.0 web character | ACF 0x20001 | 9,051 | `ae62f4c6ed914d2d9c2caea6ff39f8cde68eaca821c80116f7372e4377acbd7f` |
| `peedy.acf` | Microsoft Agent 2.0 web character | ACF 0x20001 | 7,386 | `42ad3b2ad3328020f1380e7f0696fc39834a34c6b52b67977db9a39715cb61e5` |
| `robby.acf` | Microsoft Agent 2.0 web character | ACF 0x20001 | 8,672 | `a4d3e0a4ffe4f65311b8ca8ae5ceace6e28d6c7cfacdc3fdac855653cf66bbe0` |

### 1.3 Agent 1.5 characters (OLE ACS)

`docs/research/agent15-format.md` section 3.1. The `Ozzar.acs` listed here has two damaged
animation streams (one bit error each in the compressed data, `agent15-format.md`
section 8.2).

| File | Product | Version | Bytes | SHA-256 |
|---|---|---|---|---|
| `merlin.acs` | Microsoft Agent build 1.7.0.1027 (beta), `Characters/merlin.acs` | ACF 0x1001F | 2,920,960 | `c7e8e80dd8fab1f2315f7b4b60dca12e081e35eb5dcc0b97f659819cfccc21a8` |
| `Al.acs` | third-party Agent 1.5 character | ACF 0x1001F | 2,217,472 | `437ccc7943cbc8d9364f07387c4a55c5e27b24a5ca604878c2dddb07da301cf9` |
| `Check.acs` | third-party Agent 1.5 character | ACF 0x1001F | 170,496 | `069f6a62765e43fff3bfb7092f16023544f69d571b5b2f45f2377e5946687d6e` |
| `Gourdy.acs` | third-party Agent 1.5 character | ACF 0x1001F | 426,496 | `f19dae253ad71cc64e6d152e0439f6c30945f3de5b0699ed47a9d873d0ea219c` |
| `Max.acs` | third-party Agent 1.5 character | ACF 0x1001F | 581,120 | `72b9189584b3c4925de510a3cdece910cad7eab413724517162a3be66257097a` |
| `Miku.acs` | third-party Agent 1.5 character | ACF 0x1001F | 13,824 | `3d792565767898d085323aced25cce9a37d9327fa28de2ab47720e679e11e0fc` |
| `Ozzar.acs` | third-party Agent 1.5 character | ACF 0x1001F | 4,286,464 | `0e47edee88f3cae7d0cf1c80e2869cb47c1217c8dd7c833a8ba6830ceba75165` |
| `Sharky.acs` | third-party Agent 1.5 character | ACF 0x1001E | 933,888 | `3119ae86024fb7a7c660121723ad3f23f27fdde5c70e1a139aee293eb7308260` |
| `Spaceman.acs` | third-party Agent 1.5 character | ACF 0x1001F | 442,368 | `490a5b57bf4139206525bbe7643af87ba9888bcdd7e429a234af606ec32bc580` |
| `Totem.acs` | third-party Agent 1.5 character | ACF 0x1001F | 1,033,216 | `ea2970bdb80a86e552ec9c67b08c5282ce41d43d5cbde8a26857adae1159cc40` |

### 1.4 Office 97 and Microsoft Bob actors (ACT)

A collection of 37 actor files. `docs/research/act-format.md` section 2.2 gives the inferred
origin of each file: Office 97 including the Japanese Kairu and Saeko-sensei, Greetings
Workshop, My Personal Tutor, Microsoft Bob. `ZVISIBLE.ACT` is not in the pack: it draws
nothing in any frame (a helper actor, A-PACK-3).

| File | Product | Version | Bytes | SHA-256 |
|---|---|---|---|---|
| `BAUD.ACT` | Microsoft Bob | pack 1.0 | 484,100 | `08a36be2f88dab26bb6b20d78e114f38fab0ce2777b3c136128a81bc59e0f9cb` |
| `BLYTHE.ACT` | Microsoft Bob | pack 1.0 | 356,425 | `e881d4d1c7ae1113a85cb018788012e8e63294f4e7d8c996b359cf3686508a82` |
| `CHAOS.ACT` | Microsoft Bob | pack 1.0 | 291,970 | `8836f8bbb7f36cee9e8ce1bba24b653e8efddbb1d0f06515c2cd3d87705a0d2b` |
| `CHAZ.ACT` | Microsoft Bob | pack 1.0 | 541,158 | `91814700d2d619ceda221e6a101f99439b01a27cbaf477e263efa7c6da14d0c7` |
| `CLIPPIT.ACT` | Office 97 | pack 2.2 | 468,266 | `47026de14f910ac2c51e6e509a043ac8e43fd24e0de8266e8feeeadbf65cac4b` |
| `DOLPHIN.act` | see act-format 2.2 | not recorded | 890,749 | `3c0594c1fcd2b74ffd5f5d50cdec1b7271fcf6a63e46423916d84cad2b3c1a15` |
| `DOT(Bob).ACT` | Microsoft Bob | pack 1.0 | 531,015 | `4f647cecee8ebccd000429b825612f2436e2f42000661427b80c5cd8b281fa47` |
| `DOT(Office).act` | Office 97 | pack 2.2 | 441,828 | `d9e450149b2c6ea37be33da70d165aff624f2721239b3c605a073585c465f15c` |
| `EARL.act` | see act-format 2.2 | not recorded | 2,409,895 | `f3109a7e78f7c3af68b3320b8aced3c58daaaa5465af2779b9e83919baebf19c` |
| `GENIUS.act` | Office 97 | pack 2.2 | 1,899,513 | `fff92968d040d0f1b5567624bf6faa0eeb09e2e11d48ea801e3b9c73d5e8ae4f` |
| `HANK.ACT` | Microsoft Bob | pack 1.0 | 304,572 | `254fc752165496eeeadf3555333aeb94e4892026aad40bcdaa963e11430a109c` |
| `HOPPER.ACT` | Microsoft Bob | pack 1.0 | 243,047 | `66d23ef84e1dc6e26044b53e6e7c40961ed26f565d1504c26e2e92c44cab858c` |
| `HOVERBOT.act` | Office 97 | pack 2.2 | 455,427 | `4f8916e24e5213d092c5c845f0056d31d7a0cc112f326ef7f2e6c28d9595ff24` |
| `JAVA.ACT` | Microsoft Bob | pack 1.0 | 370,865 | `291dbe23f6f3f1af01c0658546d60423125bc2351a0c82edb6f8e5548ab72974` |
| `LEXX.ACT` | Microsoft Bob | pack 1.0 | 369,561 | `3d1343b3e82c6b93ca8f005e9d1df09fa0ffafda9834750453a06b51ef7fa4a6` |
| `LOGO.act` | Office 97 | pack 2.2 | 356,778 | `37b09750ba6163b167aa273227902b2f6c27a721ca367a77d01620cfa79d8a4e` |
| `LUCY.ACT` | Microsoft Bob | pack 1.0 | 631,877 | `fd5e70e4d4549de23aa47eb4b5c411e2dd430501c3d0dbe9507cb48b11df7ae2` |
| `MNATURE.act` | Office 97 | pack 2.2 | 1,916,989 | `131b407d77e5116b5a72155a80aa3b7a53779a5871beb3e2d65a8ef5271bb2db` |
| `OFFLADY.act` | see act-format 2.2 | not recorded | 3,576,355 | `ab97f06cc0c42bfcdca09dae9f5b80c27654ad8e4928217b9fbb523ec6dcf3c8` |
| `ORBY.ACT` | Microsoft Bob | pack 1.0 | 314,350 | `8bf1eebb778d67d22e0b500630075126f3920f0c4cdbde4d85292df06be9b46e` |
| `POWERPUP.act` | Office 97 | pack 2.2 | 479,272 | `26701407f7599c3e0c600a138fadc8551a0de33bc76d42c53cef7f5aca74030d` |
| `ROBOT.act` | see act-format 2.2 | not recorded | 1,997,624 | `fcb2001c7746d17e8cbf1424edf2a03fbf890641f447637b37f92868cd1caf9b` |
| `ROCKY(EarlyGreetings).ACT` | see act-format 2.2 | not recorded | 3,431,910 | `c94ed42b47bb4c2106113eecd8001d72088fe3725580cd2ef67be526c0eadc66` |
| `ROCKY(Greetings).ACT` | see act-format 2.2 | not recorded | 2,820,438 | `02376b7ac73be74fcfb3265ae9d8193ed732918b14c915c984cc00f42ff8cc53` |
| `ROCKY(Office).act` | see act-format 2.2 | not recorded | 2,300,295 | `8757ff434aa9d5fab6f60e6690f621b408c3c5d749b3fac7ef1e7a398a883c35` |
| `ROVER.ACT` | Microsoft Bob | pack 1.0 | 429,869 | `ef4f9b9d2c69d87b8f8cb42b6af53f8516d211670a5e027d5c778ce439c31520` |
| `RUBY.ACT` | Microsoft Bob | pack 1.0 | 417,198 | `b207d1878da00f033a43f53f557c05261aa7b2eefc26f99512ea0b310195a309` |
| `SCRIBBLE.act` | Office 97 | pack 2.2 | 377,183 | `d1d3e47ef76eeaac8c481b0a8fac89410a851908cbfc4b9f7719ac888c378b44` |
| `SCUZZ.ACT` | Microsoft Bob | pack 1.0 | 413,786 | `4e6d88c1f5d5659b1150a78373e948eb933bb1b11d0ed5eea0278b151ca0741e` |
| `SHELLY.ACT` | Microsoft Bob | pack 1.0 | 489,230 | `9a66defb85d254c1def41f2eb34116f013d26be5a697fcb643ad93287ecb573d` |
| `TOFFLOGO.act` | see act-format 2.2 | not recorded | 4,120 | `e7040d39c4b9963ee380e4ca232377df71513d5fd606403d9f34d229591a4838` |
| `TUTOR.ACT` | see act-format 2.2 | not recorded | 4,601,421 | `b216ece9a23a02497fb07381effa9652ebcefb316391580374fd4d18f60e9909` |
| `WILL(Bob).ACT` | Microsoft Bob | pack 1.0 | 418,085 | `bfcc81933a17a314fe68d5d624a2a4290034807b3a40a93b805814bfc257743d` |
| `WILL(Office).act` | Office 97 | pack 2.2 | 399,965 | `00e7169818797384dd1bfc67773396bcf5559eab93c5af27072fae56a94af22d` |
| `WORM.ACT` | Microsoft Bob | pack 1.0 | 359,018 | `6a187196b1064276147632c30183f68a373da5e5307ec2bd78bcd2cd34ad6901` |
| `ZSPEAKER.ACT` | Microsoft Bob | pack 1.0 | 5,723 | `16205230b9f7d069cf371990aeac2b4e358415aae224658d81b7c9246377909b` |
| `ZVISIBLE.ACT` | Microsoft Bob | pack 1.0 | 1,847 | `99f15fa45b0c1fb69616ec6d6b419c582a52798b505ac1cc732a61fd430d3512` |

### 1.5 Office 97 Professional CD

Files of the Microsoft Office 97 Professional CD (English), `docs/research/act-format.md`
section 2.1. The nine actors are byte-identical to the ones in 1.4 and are converted once.
`MSO97.DLL` is the source of the Office palette for bitmap actors. The preview files
(`.ACP`) and `MSO7ENU.DLL` are read by the tests only.

| File | CD path | Version | Bytes | SHA-256 |
|---|---|---|---|---|
| `CLIPPIT.ACT` | `OFFICE/ACTORS/CLIPPIT.ACT` | pack 2.2 | 468,266 | `47026de14f910ac2c51e6e509a043ac8e43fd24e0de8266e8feeeadbf65cac4b` |
| `DOT.ACT` | `OFFICE/ACTORS/DOT.ACT` | pack 2.2 | 441,828 | `d9e450149b2c6ea37be33da70d165aff624f2721239b3c605a073585c465f15c` |
| `GENIUS.ACT` | `OFFICE/ACTORS/GENIUS.ACT` | pack 2.2 | 1,899,513 | `fff92968d040d0f1b5567624bf6faa0eeb09e2e11d48ea801e3b9c73d5e8ae4f` |
| `HOVERBOT.ACT` | `OFFICE/ACTORS/HOVERBOT.ACT` | pack 2.2 | 455,427 | `4f8916e24e5213d092c5c845f0056d31d7a0cc112f326ef7f2e6c28d9595ff24` |
| `LOGO.ACT` | `OFFICE/ACTORS/LOGO.ACT` | pack 2.2 | 356,778 | `37b09750ba6163b167aa273227902b2f6c27a721ca367a77d01620cfa79d8a4e` |
| `MNATURE.ACT` | `OFFICE/ACTORS/MNATURE.ACT` | pack 2.2 | 1,916,989 | `131b407d77e5116b5a72155a80aa3b7a53779a5871beb3e2d65a8ef5271bb2db` |
| `POWERPUP.ACT` | `OFFICE/ACTORS/POWERPUP.ACT` | pack 2.2 | 479,272 | `26701407f7599c3e0c600a138fadc8551a0de33bc76d42c53cef7f5aca74030d` |
| `SCRIBBLE.ACT` | `OFFICE/ACTORS/SCRIBBLE.ACT` | pack 2.2 | 377,183 | `d1d3e47ef76eeaac8c481b0a8fac89410a851908cbfc4b9f7719ac888c378b44` |
| `WILL.ACT` | `OFFICE/ACTORS/WILL.ACT` | pack 2.2 | 399,965 | `00e7169818797384dd1bfc67773396bcf5559eab93c5af27072fae56a94af22d` |
| `MSO97.DLL` | `OFFICE/MSO97.DLL` | FileVersion 8.0.3507 | 3,774,224 | `4d382905037551ef6b830a28b5b70b55043332c064665da30bae32095da1722d` |
| `MSO7ENU.DLL` | `OFFICE/MSO7ENU.DLL` | not recorded | 487,184 | `54a06d45ee45e7479e4cd7c3724f79f6393ac467e039ee003661ded72320e4e9` |
| `CLIPPIT.ACP` | `OFFICE/ACTORS/PREVIEW/CLIPPIT.ACP` | not recorded | 36,540 | `8a77f2d758447f479c289e49f5f12a0e29e0c28ea533b4afe8d178bbd9116369` |
| `DOT.ACP` | `OFFICE/ACTORS/PREVIEW/DOT.ACP` | not recorded | 10,932 | `e7db59b47059c7f3a7b8d40a1bef0af437927dd4b95606ac4ebb770d95035ae3` |
| `GENIUS.ACP` | `OFFICE/ACTORS/PREVIEW/GENIUS.ACP` | not recorded | 81,930 | `d7e424c49c0fba129d4124884067ebda6605506cdefffbc781def314b6f762d0` |
| `HOVERBOT.ACP` | `OFFICE/ACTORS/PREVIEW/HOVERBOT.ACP` | not recorded | 9,387 | `7ec150df35fd9b8f2ba1c7c30db3c4f7bbd5b5dd4e23fd39804172f76467b77d` |
| `LOGO.ACP` | `OFFICE/ACTORS/PREVIEW/LOGO.ACP` | not recorded | 3,200 | `2041881e60c57e9e8d7c37c10c51c42ad74bd446daf044288472e93558784e7f` |
| `MNATURE.ACP` | `OFFICE/ACTORS/PREVIEW/MNATURE.ACP` | not recorded | 87,942 | `d6efecb630c406d6d2190128f9024c18a5970c5567319300c0df3b4f09f4e37e` |
| `POWERPUP.ACP` | `OFFICE/ACTORS/PREVIEW/POWERPUP.ACP` | not recorded | 23,514 | `76140e487372e23ad84a92adb9dc3cb4fe4a09b3d91f645c9ed265d477882d65` |
| `SCRIBBLE.ACP` | `OFFICE/ACTORS/PREVIEW/SCRIBBLE.ACP` | not recorded | 38,225 | `4f66523d35f55e02f76d6ae79fec10e5c6461a6aa0381a3c508286263c7b31b0` |
| `WILL.ACP` | `OFFICE/ACTORS/PREVIEW/WILL.ACP` | not recorded | 41,133 | `b28aa4f2f2c984d6c24ce17d55c473bd73ba2471018b024e6036e39613044aed` |

### 1.6 Office balloon theme

The theme (`balloon-themes/office` in `bentwire-classics`) is extracted from this file.

| File | Product | Version | Bytes | SHA-256 |
|---|---|---|---|---|
| `OBALLOON.DLL` | Office XP (German) | 10.0.2607 | 132,512 | `686dfb1a491d73454691cbb5a1e272bb24348aa07c5647aa23ee26e985608f96` |

### 1.7 Agent 1.5 web characters (ACF + AAF)

Microsoft's Agent 1.5 web characters, `docs/research/agent15-format.md` section 3.2: one
folder per character (`genie`, `merlin`, `merlinsfx`, `robby`) with its ACF and the
animation files that survive, names in lower case. Read by the tests only.

| File | Product | Version | Bytes | SHA-256 |
|---|---|---|---|---|
| `genie.acf` | Microsoft Agent 1.5 web character (genie) | 0x1001F | 3,425 | `39a0171111538acff830beb68f0020d09a2fe4dc579ccca23cdc9982c21a8ff0` |
| `gestureleft.aaf` | Microsoft Agent 1.5 web character (genie) | 0x1001F | 33,744 | `fb9e36ac2168abd54a78c5cbd709028245003d97a3ae833fde5b0a16efa88406` |
| `greet.aaf` | Microsoft Agent 1.5 web character (genie) | 0x1001F | 45,079 | `a3bb033bb3dcad3482b9cdc0d402e7e0a220564f0eef378dbc37544b39106655` |
| `greetreturn.aaf` | Microsoft Agent 1.5 web character (genie) | 0x1001F | 22,084 | `b1896e1793ed341e890bc9aedd9ae3a4e38272c980572085679ab93d15903ddc` |
| `hide.aaf` | Microsoft Agent 1.5 web character (genie) | 0x1001F | 19,453 | `190be70341d74ab42d3ce8aaa4d50c4ebb6ab446587f68fd5fe4729f0b6a0382` |
| `restpose.aaf` | Microsoft Agent 1.5 web character (genie) | 0x1001F | 6,711 | `df4933a7bf10a69804a06bbf5abfd14f42ca713f99d30b8dd96207fe0beeb2c0` |
| `merlin.acf` | Microsoft Agent 1.5 web character (merlin) | 0x1001F | 3,473 | `3bbe638b4d6257270c5a9ebce13a97149b62af0c6f704acc30a24358551df2cb` |
| `blink.aaf` | Microsoft Agent 1.5 web character (merlin) | 0x1001F | 4,937 | `731e513cb23a2ad5ad10a676bf2852be6f8ac36de604ba434a68d107a141975a` |
| `show.aaf` | Microsoft Agent 1.5 web character (merlin) | 0x1001F | 26,881 | `8340f41767ff071b7ade8fd5cfe1eab2d066d425fd275d663ad8f8a93a46bd8c` |
| `merlinsfx.acf` | Microsoft Agent 1.5 web character (merlinsfx) | 0x1001F | 3,477 | `8d196dce3f7f7081fe9b24c6c3a1e20a44c21f2df3b9eb7ca63123c14778fbb3` |
| `show.aaf` | Microsoft Agent 1.5 web character (merlinsfx) | 0x1001F | 31,673 | `68e3bcfc1e60e7506b6911b1233fd68490d7e22923fd2229eee7c7eeaabe91af` |
| `robby.acf` | Microsoft Agent 1.5 web character (robby) | 0x1001F | 3,249 | `06a70f43738ea712c549421e63f4e87dfc12c3f6578afc21d298c0142d0c3469` |
| `acknowledge.aaf` | Microsoft Agent 1.5 web character (robby) | 0x1001F | 38,440 | `8725f55f1f203ff13dc2258d19d7e933a6cb400ec5e21cc81cb070984c68d900` |
| `explain.aaf` | Microsoft Agent 1.5 web character (robby) | 0x1001F | 22,130 | `4a0de52596a221f101e55a123aed6fca7c50883e3e2220fc8f85c630960d1d34` |
| `wave.aaf` | Microsoft Agent 1.5 web character (robby) | 0x1001F | 40,397 | `cc845baf72c33c0d25e4a35e8e6ffa6efb426c5725f8d9d39d4cd042de9aeb6e` |
| `wavereturn.aaf` | Microsoft Agent 1.5 web character (robby) | 0x1001F | 27,938 | `446bb94fc69b67c2b70cb58bb49edb44c2e91abd010513fb631fac229c31fb93` |

## 2. Analysed Microsoft binaries

All were analysed statically and never executed. Sizes and SHA-256 values of files inside an
archive were computed from the extracted file. The values for the two Agent 1.5 `agentdpv.dll`
files, `MSagent.exe` and `ACE.EXE` are quoted from `docs/research/`; the English
`OBALLOON.DLL` was not kept.

| Binary | Version | Bytes | SHA-256 | Product |
|---|---|---|---|---|
| `agentsvr.exe` | 2.00.0.3427 | 256,512 | `b2aaa8618528037537fef6280d05ff3f88b2b50fc250ca04e7d4bf3fa970e7dd` | Microsoft Agent 2.0 runtime |
| `agentdp2.dll` | 2.00.0.3427 | 42,496 | `77eb0d33243b92ba6e550df31f874f576a7e69c66687123c6514a641fe98aae1` | Microsoft Agent 2.0 runtime |
| `agentdpv.dll` | 2.00.0.3427 | 57,344 | `66a6765e2693b05c7b1bb72973b32264656ffbba7c176f8a86ce0edc8358875c` | Microsoft Agent 2.0 runtime |
| `agentanm.dll` | 2.00.0.3427 | 24,064 | `4f1cb8b500671be72448e6bfa51821168c99f56f25237d4ce9f87d052b9ce934` | Microsoft Agent 2.0 runtime |
| `agentctl.dll` | 2.00.0.3427 | 214,016 | `b50ca2fdd6c41b9c7637c486a49f044286f19f83b7d9fc97cce3eb2e21ae315c` | Microsoft Agent 2.0 runtime |
| `AgentDPv.dll` (Agent 1.5) | 1.5.2.416 | not recorded | `4a5de7cc6327db9e8a610451b91343aa24f270e9f7050808087d0d9760a7b6d1` | Microsoft Agent 1.5 runtime installer `MSagent.exe` (340,112 bytes, SHA-256 `5e00fe1b303dc56e3cf556adaf23a956ccf2ab2bb42b1a50736ad5bd4f3d5208`) |
| `agentdpv.dll` (Agent 1.5) | 1.5.0.225 | not recorded | `039e856fa12f9d029e5faa2de7c7bd0ad00807ed3c74ae09d429c26f489da506` | Microsoft Agent build 1.5.0.225 |
| `mslwvtts.dll` | not recorded (part of the 2.00.0.3427 runtime) | 39,936 | `2cd78839b82fd0c1f170f7a9fa7d1a6d44bea1564370f337f7c0ef1baac0d9da` | Microsoft Agent 2.0 runtime |
| `intl/agt0409.dll` (English UI strings) | not recorded (part of the 2.00.0.3427 runtime) | 19,968 | `fb0dcabdb076b8f81a5416c107e5ced3222cce4a1c2135f0068e3410be15ad88` | Microsoft Agent 2.0 runtime |
| `intl/agt0407.dll` (German UI strings) | not recorded (part of the 2.00.0.3427 runtime) | 21,504 | `4e2503e1281f9530c0d83d3692d867a4c4c794003e69e8121fc6676d2e320010` | Microsoft Agent 2.0 runtime |
| `agtctl15.tlb` (type library of the 1.5 control) | not recorded (part of the 2.00.0.3427 runtime) | 18,432 | `7bdbbe55f8f84cdec639e53a427229f73d507cee22a0c65a8bb2009e3807b4a2` | Microsoft Agent 2.0 runtime |
| `ACE.EXE` (Agent Character Editor, September 1998) | not recorded | not recorded | `1f2856e79f764eb8a6dfb259cc47240e929db412e10ba512790d69367cef20a2` | Microsoft Agent Character Editor (Agent 2.0 SDK, self-extracting CAB) |
| `msadp32.acm` | 5.1.2600.5512 (Windows XP SP3, German) | 14,848 | `81eac2425efa2c31edf22d4c8a87e9fdadeb380b2471779c508f00145c5dd243` | Windows XP SP3 (German) |
| `MSO.DLL` (Office XP) | 10.0.2625 | 9,995,680 | `14b7a79ad2cd96524671f8a63d63ed665253efcf02da74a814fec83f33225134` | Office XP |
| `MSO97.DLL` (Office 97) | 8.0.3507 | 3,774,224 | `4d382905037551ef6b830a28b5b70b55043332c064665da30bae32095da1722d` | Office 97 Professional CD, see 1.5 |
| `OBALLOON.DLL` (German) | 10.0.2607 | 132,512 | `686dfb1a491d73454691cbb5a1e272bb24348aa07c5647aa23ee26e985608f96` | Office XP (German), see 1.6 |
| `OBALLOON.DLL` (English, 1033) | 10.0.2607 | not recorded | not recorded | Office XP CD (English), `OFFICE1.CAB` (read for the English button captions only, `docs/spec.md`) |
| `BLNMGR.DLL` | 10.0.2607 | 79,264 | `aa43a71da074aa1a0fe249a699d23e35c724f84ae2925a9e4291812591e0701f` | Office XP |
| `MSOINTL.DLL` (German, 1031) | 10.0.2609 | 1,234,336 | `b23bc219d060169bd35df2c7003c1266b3fe4777dd9299f8888b02cfc6768843` | Office XP (German) |

Agent build 1.7.0.1027 (`AgentDpv.dll`) was available but not analysed.

## 3. Method

The behaviour of the original runtimes and the layout of their files were described in our
own words from static analysis of the binaries above (disassembly and decompilation; nothing
was executed) and from the sample files, for the purpose of interoperability with these file
formats (EU Directive 2009/24/EC, Article 6). No Microsoft code is copied into bentwire, and
the research documents reproduce no decompiled code. bentwire's code was written from `docs/`
and the material that `docs/architecture.md` section 3 allows; the disassembly and
decompilation outputs are not part of any repository.
