# Open Source Avatars — mitgelieferte VRM-Charaktere

Diese zehn Dateien stammen aus der öffentlich zugänglichen Open-Source-Avatars-Datenbank und sind zusätzlich im App-Paket enthalten. Sowohl die Sammlungsdaten als auch die jeweiligen eingebetteten VRM-Metadaten nennen CC0. Die Dateien wurden umbenannt, ihr Binärinhalt wurde nicht verändert. Die übrige Anwendung erhält dadurch keine neue Lizenz.

Quelle: https://www.opensourceavatars.com/en/finder
Sammlungen und Lizenzangaben: https://github.com/ToxSam/open-source-avatars/blob/main/data/projects.json
Lizenz: [CC0 1.0 Universal](https://creativecommons.org/publicdomain/zero/1.0/) · [vollständiger Text](CC0-1.0.txt)

## Herkunft und Dateiprüfsummen

| Figur | Sammlung / Urheberangabe im Register | App-Datei | SHA-256 |
| --- | --- | --- | --- |
| Mushy | 100Avatars R1 / Polygonal-Mind | `Mushy.vrm` | `82d3b4f6efd50d41f6f985720e7405a391beb7acf16439adf89b22805cb3333f` |
| Dracula | 100Avatars R1 / Polygonal-Mind | `Dracula.vrm` | `4e6442065696c1058b99a54b1ddda3457126492e69a8a92e8cd9f92d0bea165c` |
| AlwaysWatching | 100Avatars R1 / Polygonal-Mind | `AlwaysWatching.vrm` | `e128a213537d615e951d393ee6e4e31063555f2d7ba3fa607e25b490070d1612` |
| ConeheadBeing | 100Avatars R2 / Polygonal-Mind | `ConeheadBeing.vrm` | `204330e54aa0393c7b679de487c65fcc5da337ec745e8780511f6c2a6bf5d0d3` |
| CoolAlien | 100Avatars R1 / Polygonal-Mind | `CoolAlien.vrm` | `4ef6749ac4dcf3fa6d7c35f676774bf2f79c1876b8db3a42fc68f2bb01d96acf` |
| CoolTiger | 100Avatars R3 / Polygonal-Mind | `CoolTiger.vrm` | `4b471b59517733b2f3804acc80e8ea485260efb956a5c950221e4e1ff30860e2` |
| GhostPumpking | 100Avatars R3 / Polygonal-Mind | `GhostPumpking.vrm` | `a654eda4fa660b39e6c53f93e066a4e5d998c4cbeb709214ef1a6a3a8d825f64` |
| MaxHax | ToxSam / toxsam | `MaxHax.vrm` | `0d56807ca0cd5b0fef1c7080b5a70a45e4c710f41e2c88998ced3a195bbca0c9` |
| MeganTheFox | 100Avatars R3 / Polygonal-Mind | `MeganTheFox.vrm` | `7167d50eb9fae3d3e0453ab6ab2e26b777452b5e0dc41108dfa162a3a061efd2` |
| Paco | 100Avatars R3 / Polygonal-Mind | `Paco.vrm` | `782e193b56e2c3b8a3b9119c2c0739c0c12c14be599c1939c3e047a85eee851d` |

Die Register-Sammlungen 100Avatars R1/R2/R3 nennen `Polygonal-Mind` als Ersteller. Bei R1 und R2 lautet auch der VRM-Autor `Polygonal Mind`. Die neueren R3-Dateien enthalten teilweise eine Wallet beziehungsweise einen Deployer als Autor; diese Angaben werden unverändert separat in [CREDITS.json](CREDITS.json) erhalten und nicht einer Person zugeordnet. MaxHax nennt `ToxSam` als eingebetteten Autor. Dort stehen außerdem die exakten Original- und Download-URLs, vollständige VRM-Rechtemetadaten und zusätzliche Lizenzverweise.

## Animationen und Nutzung in der App

Alle zehn Dateien enthalten ein Humanoidrig mit Kopf, Wirbelsäule und beiden Oberarmen sowie Vokalausdrücke und Blinzeln. Sie enthalten keine eigenen Animationsclips. Die App erzeugt Winken, Verstecken, Freuen, Nicken und Tanzen über ihre vorhandene prozedurale VRM-Steuerung. Mundbewegungen folgen der Audioamplitude über die vorhandenen Vokalmorphs; das ist keine phonetische Einzelwortausrichtung.

Geometrie, Texturen und benötigte Buffer sind in den VRM-Dateien eingebettet. Nach der Installation ist für die Charakterdarstellung kein externer Avatar-Download erforderlich. Diese Feststellungen beruhen auf den statisch gelesenen glTF-/VRM-Metadaten; es wurden keine Darstellungs-, Laufzeit- oder Leistungstests ausgeführt.
