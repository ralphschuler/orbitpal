# Figuren, Bewegungen und Gesichtsausdrücke

Du wählst eine mitgelieferte Figur oder importierst eine `.vrm`-, `.glb`- oder `.gltf`-Datei. Die App erkennt das Format, vorhandene Bewegungen, Materialien und Gesichtsdaten selbst und richtet die Haltung automatisch ein. Manuelle Armausrichtung und Pose-Kalibrierung gehören nicht zum Bedienablauf.

Eine Figur kann nur die Formen und Bewegungen verwenden, die ihre Datei enthält oder die sich aus ihrem vorhandenen Skelett ableiten lassen. Fehlende Mundformen, Animationen oder Tonspuren werden nicht als vorhandene Funktionen ausgegeben. Diese Übersicht beschreibt den implementierten Umfang; beliebige Erweiterungen und beschädigte Fremddateien können zusätzliche Unterstützung benötigen.

## Was automatisch unterstützt wird

| Datei oder Funktion | Status | Verhalten |
| --- | --- | --- |
| Nova | Unterstützt | Mitgelieferte, direkt erzeugte Figur mit eigener Gestik und animiertem Mund. |
| VRM 0.x | Unterstützt, mit einzelnen Einschränkungen | Das ältere VRM-Format mit `VRM`-Erweiterung und `specVersion: 0.0` wird automatisch erkannt. Körper, Materialien, Gesichtsausdrücke, Blicksteuerung und bewegliche Haare/Kleidung werden über den VRM-Loader verarbeitet. |
| VRM 1.0 | Unterstützt | `VRMC_vrm` mit Version `1.0` sowie die vom Loader unterstützte frühere `1.0-beta` werden erkannt. Die App nutzt die standardisierte Körper- und Gesichtszuordnung; unterschiedliche Knochenachsen werden automatisch behandelt. |
| GLB mit glTF 2.0 | Unterstützt | Geometrie, Texturen, Materialien, Skelett, Formveränderungen und eingebettete Animationsclips werden geladen. Die passende Ruhebewegung wird bevorzugt; Emotes können vorhandene Clips verwenden. |
| glTF 2.0 JSON (`.gltf`) | Unterstützt mit lokalen Begleitdateien | Relative BIN-Dateien und PNG/JPEG/BasisU-KTX2-Texturen einschließlich Base64-Daten werden beim Import zu einem vollständigen GLB gebündelt. Skelett, Formveränderungen und enthaltene Animationsclips bleiben erhalten. |
| Vorhandene Körperanimationen | Unterstützt | Tatsächliche Clipnamen und passende bekannte Bewegungen werden erkannt. Eigene Clips behalten ihre Körper- und Gesichtsspuren; automatische Gestik soll sie nicht überschreiben. Ein Clip gehört zu seinem Modell und wird nicht beliebig auf ein anderes Skelett übertragen. |
| VRM-Gesicht und Mund | Unterstützt, abhängig von der Datei | Alte und neue Standardnamen für Emotionen und Mundformen werden zugeordnet. Beim Sprechen folgen vorhandene Mundformen dem Audiosignal; bei Systemstimmen dienen echte Start-/Ende- und Wortereignisse als Grundlage. Das ist eine Sprechannäherung, keine genaue Lautanalyse. |
| Gesichtsdaten in gewöhnlichen GLB-Dateien | Eingeschränkt | Erkannte Mund-, Kiefer- und Gesichtselemente werden verwendet. GLB legt keine allgemeine Bedeutung für beliebige eigene Gesichtsnamen fest. Ohne passende Mundformen oder Kiefer bleibt die Figur beim Sprechen ohne künstliche Körperbewegung. |
| PBR, Unlit und VRM-MToon-Materialien | Unterstützt im Loader-Umfang | Eingebettete Standardtexturen und die eingerichteten Materialerweiterungen werden verarbeitet. Hersteller-Shader, besondere Materialanimationen oder andere nicht eingerichtete Erweiterungen sind nicht automatisch abgedeckt. |
| Eigene Emote-Sounds | Unterstützt, separat zugeordnet | Ein Emote kann einen eingebauten oder importierten Sound erhalten. Dieser startet mit der Geste; Lautstärke, Stop und Ausgabemodus gelten auch für ihn. |
| Audio in einer Modelldatei | Explizites MSFT-Draft-Teilprofil | Eingebettetes PCM16-WAV mit Play/Stop-Animationsereignissen, festen Budgets und ausgewählter, ausdrücklich aktivierter Ausgabe. Keine automatische Wiedergabe bei Load/Idle; Vorschauen bleiben stumm. Andere erforderliche Erweiterungen werden abgelehnt, optionale mit Hinweis stumm geschaltet. [Profil und Grenzen](AUDIO-EXTENSIONS.md). |
| Separate VRM-Animation (`.vrma`) | Unterstützt als eigener Bewegungsimport | VRMA 1.0 wird pro Bot importiert und über den offiziellen Retargeter auf VRM 0/1 übertragen. Körper, Ausdrücke und Blickspuren behalten ihre natürliche Dauer. GLB-/Nicht-Humanoid-Ziele werden nicht unterstützt. [Umfang und Grenzen](VRMA-MOTIONS.md). |

VRM baut auf dem binären glTF-2.0-Format auf. VRM 0.x und 1.0 unterscheiden sich unter anderem in der Körper- und Gesichtszuordnung; diese Unterschiede verarbeitet die Software. Die offiziellen Beschreibungen erläutern [VRM 0.x](https://github.com/vrm-c/vrm-specification/tree/master/specification/0.0), [VRM 1.0](https://github.com/vrm-c/vrm-specification/tree/master/specification/VRMC_vrm-1.0) und die [VRM-Funktionen](https://vrm.dev/en/vrm/vrm_features/).

## Grenzen beim Import

VRM- und GLB-Dateien müssen vollständige GLB-2-Container mit glTF 2.0 sein und ihre benötigten Daten selbst enthalten. Beim neuen `.gltf`-Import darf die JSON-Datei relative BIN-Dateien und PNG/JPEG/BasisU-KTX2-Texturen in ihrem eigenen Ordner oder Unterordnern referenzieren. Das Paket zuerst vollständig entpacken und die Ordnerstruktur beibehalten. Die App liest die benötigten Ressourcen lokal und erstellt daraus ein eingebettetes GLB. Bereits vorhandene Bild-bufferViews und passende Base64-Daten-URIs werden ebenfalls verarbeitet. Profilexporte nehmen anschließend dieses portable GLB mit. Der Avatar-Zielspeicher muss Hardlinks unterstützen; andernfalls bricht der Import mit Erklärung ab, bevor ein unvollständiges Modell veröffentlicht wird.

Die Grenze beträgt **256 MiB für das fertige GLB**, **8 MiB für dessen JSON-Beschreibung** und beim `.gltf`-Import **32 MiB für die JSON-Quelldatei**. Höchstens 1024 Puffer/Bilder und PNG/JPEG/BasisU-KTX2-Texturen bis 64 MiB, 16384 Pixel Kantenlänge und 64 Megapixel werden angenommen. Die Prüfung von Dateikopf und Abmessungen ersetzt keine vollständige Bilddekodierung.

Es werden keine HTTP-/Datei-URLs, absoluten Pfade, `..`-Verweise, Symlinks oder umgeleiteten Ressourcen geladen. Quelldateien müssen unverändert im ausgewählten Modellordner erreichbar sein. Der glTF-JSON-Import verarbeitet unkomprimierte oder Draco-/Meshopt-Puffer sowie PNG/JPEG/BasisU-KTX2-Texturen; WebP und AVIF vorher konvertieren. Bestehende GLB/VRM-Dateien verwenden weiterhin ihren bisherigen Loader-Umfang. Fehlerhafte, unvollständige oder unbekannte neuere Formatversionen werden mit einer Erklärung abgelehnt.

FBX, OBJ, Collada/DAE und `.blend` haben derzeit keinen direkten Modellimport. VRMA wird separat als Bewegung für VRM-Figuren importiert. Passende Modelle können in Blender als eingebettetes GLB exportiert werden. Konvertierung erzeugt keine fehlenden Knochen, Mundformen oder Bewegungsclips. Weitere Bezugsquellen und Exportwege stehen in [Charaktere erstellen und finden](CHARACTER-SOURCES.md).

Draco-, KTX2/BasisU- und EXT/KHR-Meshopt-Decoder werden passend zur installierten Three-Version lokal mitgeliefert und in Studio, nativen Overlays und OBS gemeinsam verwendet. Bei KTX2 wird ein zum Grafikgerät passendes Texturformat gewählt; Details, Grenzen und Offline-Prüfungen stehen unter [Komprimierte Figuren](COMPRESSED-AVATARS.md). Auch andere unbekannte **erforderliche** Erweiterungen führen zu einem Fehler. Eine unbekannte **optionale** Erweiterung kann mit einer Warnung übergangen werden, wenn das Modell einen verwendbaren Standardinhalt bietet. Das entspricht der Unterscheidung zwischen erforderlichen Erweiterungen und optionalen Ersatzdarstellungen im [glTF-Erweiterungsregister](https://github.com/KhronosGroup/glTF/blob/main/extensions/README.md).

Der eingesetzte Renderer verwendet **Three.js 0.186.1** und **`@pixiv/three-vrm` 3.5.5**, wie im [Lockfile](../pnpm-lock.yaml) festgehalten. Die oben genannten Kompressionsdecoder werden ausdrücklich lokal gebündelt; weitere vom allgemeinen [GLTFLoader](https://threejs.org/docs/pages/GLTFLoader.html) angebotene Erweiterungen sind dadurch nicht automatisch eingerichtet. Einzelne ältere VRM-Sonderfälle bleiben eingeschränkt: Der VRM-Loader wertet alte Blicksteuerungskurven nicht vollständig aus; besondere Transparenzreihenfolgen alter MToon-Materialien können anders aussehen. Eine Ladeprüfung ersetzt keine visuelle Kontrolle jedes fremden Modells.

## Was in den 17 mitgelieferten Figuren steckt

Die Bestandsaufnahme vom 19.10.2026 prüft den Katalog und die binären Modelldaten. **17 Figuren entsprechen einer direkt erzeugten Nova und 16 Modelldateien.** Alle 16 Dateien verwenden den GLB-2-Container mit glTF 2.0, enthalten ihre Ressourcen selbst und deklarieren keine erforderlichen Erweiterungen.

| Figur | Modelltyp | Eingebettete Animationsclips | Gesichtsdaten laut Datei |
| --- | --- | ---: | --- |
| Nova | Direkt erzeugt | Eigene App-Bewegungen | Eigener animierter Mund und Augen |
| Robot Expressive | GLB | 14 | Benannte emotionale Gesichtsformen |
| Fox | GLB | 3 | Keine standardisierte VRM-Gesichtszuordnung |
| Ritter | GLB | 76 | Keine standardisierte VRM-Gesichtszuordnung |
| Magier | GLB | 76 | Keine standardisierte VRM-Gesichtszuordnung |
| Schurke | GLB | 76 | Keine standardisierte VRM-Gesichtszuordnung |
| Skelett | GLB | 95 | Keine standardisierte VRM-Gesichtszuordnung |
| Mushy | VRM 0.0 | 0 | 6 Ausdrucksgruppen |
| Dracula | VRM 0.0 | 0 | 6 Ausdrucksgruppen |
| AlwaysWatching | VRM 0.0 | 0 | 6 Ausdrucksgruppen |
| ConeheadBeing | VRM 0.0 | 0 | 8 Ausdrucksgruppen |
| CoolAlien | VRM 0.0 | 0 | 6 Ausdrucksgruppen |
| CoolTiger | VRM 0.0 | 0 | 17 Ausdrucksgruppen |
| GhostPumpking | VRM 0.0 | 0 | 17 Ausdrucksgruppen |
| MaxHax | VRM 0.0 | 0 | 17 Ausdrucksgruppen |
| MeganTheFox | VRM 0.0 | 0 | 17 Ausdrucksgruppen |
| Paco | VRM 0.0 | 0 | 17 Ausdrucksgruppen |

Die sechs GLB-Figuren enthalten zusammen **340 Animationsclips**. Die zehn älteren VRM-Figuren enthalten Körperzuordnungen und Gesichtsausdrücke, aber keine gespeicherten Bewegungsclips; ihre Gestik entsteht automatisch in der App. Eine Ausdrucksgruppe beschreibt vorhandene Modelldaten und garantiert nicht jede denkbare Mimik. Im Katalog ist keine VRM-1.0-Datei enthalten; Unterstützung und Prüfungen dieses Formats müssen deshalb zusätzlich durch Referenzdateien und lokale Fixtures abgedeckt werden.

**Keine der 16 Modelldateien enthält deklarierte Audioerweiterungen.** Die vorhandenen Bewegungen bringen somit keine in diesen Dateien ausgewiesenen Tonspuren mit. Die Zuordnung eines eigenen Emote-Sounds bleibt unabhängig vom Avatarformat. Quellen und Nutzungsrechte stehen in [CHARACTERS.md](../CHARACTERS.md) und den [Asset-Nachweisen](../public/characters/ATTRIBUTION.md).

## Laufbewegung und Sprechen

Geeignete mitgelieferte Laufclips werden im [Autopiloten](AUTOPILOT.md) automatisch bevorzugt. Die sechs enthaltenen GLB-Modelle bringen solche Clips bereits mit. Nova und die humanoiden OSA-VRMs ohne eigene Clips verwenden einen automatisch berechneten Gehzyklus. Unbekannte Skelette bekommen ohne geeigneten Clip keine vorgetäuschte Gehfähigkeit. Kamerabereich und Modellgröße begrenzen den Weg; dafür gibt es keine Armkalibrierung.

Sprache steuert Mundformen beziehungsweise einen vorhandenen Kiefer, keine künstlichen Kopf-, Arm- oder Körper-Sprechbewegungen. Ein Modell ohne geeignete Mundformen oder Kiefer kann deshalb nicht durch bloßes Körperwackeln wie ein sprechender Charakter aussehen. Eigene Ruheanimationen, Blinzeln und bewusst gewählte Emotes bleiben unabhängig davon erhalten.

## Warum separate Animationen und Modell-Audio eigene Funktionen brauchen

Die offizielle [VRM-Animation-1.0-Spezifikation](https://github.com/vrm-c/vrm-specification/blob/master/specification/VRMC_vrm_animation-1.0/README.md) beschreibt eine separate, gewöhnlich modellfreie Datei für Körperbewegung, Gesicht und Blick. Sie ist ausdrücklich für getrennte Animationen vorgesehen. OrbitPal verwendet das passende offizielle [three-vrm-animation-Paket](https://pixiv.github.io/three-vrm/docs/modules/three-vrm-animation.html) für den eigenen VRMA-Import und die Übertragung auf eine vorhandene VRM-0/1-Figur. GLB-Figuren und Nova sind keine VRMA-Ziele; die Datei ergänzt keine fehlenden Knochen, Mundformen oder Tonspuren.

Audio ist keine einheitliche Emote-Tonspur des glTF- oder VRM-Kernformats. OrbitPal unterstützt das versionsgebundene Teilprofil `orbitpal-msft-audio-v1` eines fest gepinnten Microsoft-Entwurfs mit eingebetteten WAV-Clips und Animationsereignissen. Andere Vorschläge werden dadurch nicht automatisch unterstützt. [Ressourcenregeln, Nutzeraktivierung, Ausgabe, Lautstärke, Stop und Prüfgrenzen](AUDIO-EXTENSIONS.md) sind Teil dieses Profils.

[VRMA-Bewegungen](VRMA-MOTIONS.md) sind als separater Arbeitsablauf verfügbar; [Kompressionsdecoder](COMPRESSED-AVATARS.md) werden lokal mitgeliefert. [Modell-Audio](AUDIO-EXTENSIONS.md) beschränkt sich auf das ausdrücklich beschriebene Entwurfs-Teilprofil und ergänzt keine Tonspuren in Dateien, die keine enthalten.
