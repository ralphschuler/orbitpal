# Weitere 3D-Charaktere

Recherche vom **18. Oktober 2026**. Diese Übersicht unterscheidet mögliche Importquellen von tatsächlich mitgelieferten Dateien. Für die Auswahl wurden keine Accounts angelegt oder Anbieterprogramme installiert. Die mitgelieferten Fremdmodelle liegen lokal im App-Paket.

## Mitgelieferte Auswahl

| Charakter | Umfang | Herkunft und Lizenz |
| --- | --- | --- |
| **Nova** | Eigene prozedurale Roboterfigur mit Mundanimation und Gesten. | Originalfigur dieses Projekts; keine zusätzliche Modelldatei nötig. |
| **Robot Expressive** | GLB-Roboter mit enthaltenen Animationen und Gesichtsmorphs. | Tomás Laulhé / Quaternius; Änderungen von Don McCurdy; CC0 1.0. |
| **Fox** | GLB-Fuchs mit Umsehen-, Geh- und Laufanimationen. Kein Mund- oder Gesichtsrig. | PixelMannen-Modell: CC0; Rig/Animation von tomkranis und glTF-Konvertierung von AsoboStudio/scurest: CC BY 4.0. |
| **KayKit Knight / Ritter** | Humanoider Ritter mit enthaltenen Skelettanimationen. | Kay Lousberg, Adventurers Character Pack 1.0; CC0 1.0. |
| **KayKit Mage / Magier** | Humanoider Magier mit enthaltenen Skelettanimationen. | Kay Lousberg, Adventurers Character Pack 1.0; CC0 1.0. |
| **KayKit Rogue / Schurke** | Humanoider Schurke mit enthaltenen Skelettanimationen. | Kay Lousberg, Adventurers Character Pack 1.0; CC0 1.0. |
| **KayKit Skeleton / Skelett** | Skelettfigur mit enthaltenen Skelettanimationen. | Kay Lousberg, Skeletons Character Pack 1.0; CC0 1.0. |
| **Mushy** | VRM-Humanoid mit Vokalmorphs, Blinzeln und App-Gesten. | 100Avatars R1 / Polygonal Mind; CC0. |
| **Dracula** | VRM-Humanoid mit Vokalmorphs, Blinzeln und App-Gesten. | 100Avatars R1 / Polygonal Mind; CC0. |
| **AlwaysWatching** | VRM-Humanoid mit Vokalmorphs, Blinzeln und App-Gesten. | 100Avatars R1 / Polygonal Mind; CC0. |
| **ConeheadBeing** | VRM-Humanoid mit Vokalmorphs, Blinzeln und App-Gesten. | 100Avatars R2 / Polygonal Mind; CC0. |
| **CoolAlien** | VRM-Humanoid mit Vokalmorphs, Blinzeln und App-Gesten. | 100Avatars R1 / Polygonal Mind; CC0. |
| **CoolTiger** | VRM-Humanoid mit Vokalmorphs, Blinzeln und App-Gesten. | 100Avatars R3 / Polygonal-Mind als Sammlungsersteller; eingebetteter Autor separat dokumentiert; CC0. |
| **GhostPumpking** | VRM-Humanoid mit Vokalmorphs, Blinzeln und App-Gesten. | 100Avatars R3 / Polygonal-Mind als Sammlungsersteller; eingebetteter Autor separat dokumentiert; CC0. |
| **MaxHax** | VRM-Humanoid mit Vokalmorphs, Blinzeln und App-Gesten. | ToxSam; CC0. |
| **MeganTheFox** | VRM-Humanoid mit Vokalmorphs, Blinzeln und App-Gesten. | 100Avatars R3 / Polygonal-Mind als Sammlungsersteller; eingebetteter Autor separat dokumentiert; CC0. |
| **Paco** | VRM-Humanoid mit Vokalmorphs, Blinzeln und App-Gesten. | 100Avatars R3 / Polygonal-Mind als Sammlungsersteller; eingebetteter Autor separat dokumentiert; CC0. |

Die [vollständigen Herkunfts- und Attributionshinweise](public/characters/ATTRIBUTION.md) enthalten Quellen, Dateiprüfsummen, Lizenztexte und einen kopierbaren Credittext für Fox. Insbesondere die animierte Fox-Datei ist nicht ausschließlich CC0. Die Fremdmodelle sind in `public/characters/` enthalten; die Auswahl erzeugt keinen automatischen Download.

Die vier KayKit-Dateien stammen aus den offiziellen Repositorys [Adventurers 1.0](https://github.com/KayKit-Game-Assets/KayKit-Character-Pack-Adventures-1.0) und [Skeletons 1.0](https://github.com/KayKit-Game-Assets/KayKit-Character-Pack-Skeletons-1.0). Die dortigen `LICENSE.txt`-Dateien erklären CC0 und erlauben persönliche, Bildungs- und kommerzielle Projekte. Kopien liegen als [Adventurers-Lizenz](public/characters/KayKit-Adventurers-LICENSE.txt) und [Skeletons-Lizenz](public/characters/KayKit-Skeletons-LICENSE.txt) bei. Diese fertigen GLB-Dateien benötigen weder einen Avatar-Account noch eine FBX-Konvertierung. Körperanimationen bedeuten dabei nicht automatisch ein geeignetes Gesichts- oder Lippenrig.

Die Auswahl umfasst insgesamt **17 direkt verfügbare Charaktere**. Die zehn zusätzlichen VRMs stammen aus dem [Open Source Avatars Finder](https://www.opensourceavatars.com/en/finder). CC0 ist sowohl in den [Sammlungsdaten](https://github.com/ToxSam/open-source-avatars/blob/main/data/projects.json) als auch in den eingebetteten VRM-Lizenzangaben vermerkt. Ihre Dateien liegen unter `public/characters/osa/`; [Herkunft, Nutzungsumfang und Prüfsummen](public/characters/osa/ATTRIBUTION.md) sowie die [vollständigen eingebetteten Autoren- und Lizenzmetadaten](public/characters/osa/CREDITS.json) werden mitgeliefert. Einzelne Autorenfelder enthalten Wallet-/Deployer-Angaben; diese werden keiner Person zugeordnet.

Diese zehn VRMs besitzen keine eigenen Animationsclips. Winken, Verstecken, Freuen, Nicken und Tanzen entstehen über die vorhandene prozedurale Humanoidsteuerung des Cohosts. Vorhandene Vokalmorphs werden zur Audioamplitude bewegt. Die Dateien wurden unverändert übernommen; eine genaue phonetische Lippensynchronität oder gemessene Leistung wird damit nicht zugesagt.

## Importformat

Der Cohost importiert **`.vrm` und `.glb`**. Eine `.vroid`-Datei ist ein bearbeitbares VRoid-Projekt und muss zuerst als VRM exportiert werden. FBX, OBJ und Blender-Projekte müssen vorher nach GLB oder VRM konvertiert werden. Bei einem mehrteiligen glTF-Download müssen Geometrie, Bilder und gegebenenfalls Animationen in eine eigenständige GLB-Datei gepackt werden.

Ein erfolgreich geladener Körper garantiert keine Lippenbewegungen oder Gesten: Dafür benötigt das Modell passende Gesichtsausdrücke/Morph Targets, ein geeignetes Skelett oder enthaltene Animationsclips. VRM bietet eine standardisierte Humanoid- und Ausdrucksstruktur; bei allgemeinem GLB hängt der Umfang vom Modell ab.

## Quaternius: geeignete zusätzliche Quellen

| Quelle | Nachweis auf der Herstellerseite | Verwendung im Cohost |
| --- | --- | --- |
| [Universal Base Characters](https://quaternius.com/packs/universalbasecharacters.html) | CC0; humanoides Rig; FBX und glTF. Die Seite unterscheidet eine kostenlose Teilmenge vom erweiterten Source-Angebot. | Gute Grundlage für individuell gestaltete humanoide Bots. Den tatsächlich enthaltenen glTF-Charakter als GLB aufbereiten; Gesichtsanimationen sind separat zu prüfen. |
| [Pirate Kit](https://quaternius.com/packs/piratekit.html) | CC0; animierte Figuren und Umgebung; FBX, OBJ, Blend und glTF. | Geeignet für einen thematischen Piraten-Cohost. Nur die gewünschten Charakterdateien und benötigten Ressourcen übernehmen und zu GLB zusammenführen. |

Beide Seiten nennen die Nutzung in persönlichen und kommerziellen Projekten. Die CC0-Angabe bezieht sich auf das jeweilige Quaternius-Angebot; fremde zusätzliche Kleidung, Texturen oder Animationen erhalten dadurch keine neue Lizenz. Quaternius-Dateien sind hier als weitere Quellen aufgeführt und noch keine nachgewiesen eingebauten Charaktere.

Weitere kostenlose Alternativen sind [Kenney Animated Characters Protagonists](https://kenney.nl/assets/animated-characters-protagonists) unter CC0 sowie die freien Standardversionen von [KayKit Adventurers](https://kaylousberg.itch.io/kaykit-adventurers) und [KayKit Skeletons](https://kaylousberg.itch.io/kaykit-skeletons). Kostenpflichtige EXTRA- und SOURCE-Versionen werden für die mitgelieferte Auswahl nicht benötigt. Bei einem eigenen zusätzlichen Download muss das tatsächliche Dateiformat weiterhin zum Import passen.

## Eigene Figur aus VRoid Studio

[VRoid Studio](https://vroid.com/en/studio) kann eigene Figuren als VRM exportieren. Das ist der direkte Weg zu einem humanoiden Charakter mit eigenem Erscheinungsbild. Die offizielle Seite verweist bei pixiv- und Drittanbieterbestandteilen ausdrücklich auf deren jeweilige Nutzungsbedingungen. Eine zusätzliche Installation wird vom Cohost nicht automatisch vorgenommen.

[VRoid Hub](https://developer.vroid.com/en/guidelines/conditions_of_use.html) enthält Modelle mit individuellen Freigaben. Vor einem Import müssen insbesondere Avatar-Nutzung, individuelle beziehungsweise gewerbliche Nutzung, Änderungen, Weitergabe und erforderliche Credits zum konkreten Modell passen. Eine Möglichkeit zur Anzeige oder Nutzung innerhalb einer integrierten Hub-App bedeutet nicht automatisch, dass eine frei herunterladbare VRM-Datei verfügbar ist. Eine Hub-Kontoanbindung ist im Cohost nicht implementiert.

Auch offizielle VRoid-Beispielmodelle sind nicht pauschal CC0: Die [VRoid-FAQ zu Samples](https://vroid.pixiv.help/hc/en-us/articles/4402614652569-Do-VRoid-Studio-s-sample-models-come-with-conditions-of-use) unterscheidet CC0-Beispiele von Modellen mit besonderen Bedingungen. Maßgeblich bleiben die Angaben des konkret gewählten Modells.

## Offizielle VRM-Beispiele

Das offizielle [VRM1 Constraint Twist Sample](https://github.com/vrm-c/vrm-specification/blob/master/samples/VRM1_Constraint_Twist_Sample/README.md) nennt **VRM Public License 1.0**, © 2022 pixiv Inc. Diese Modelllizenz ist von der Softwarelizenz des verwendeten Renderers zu unterscheiden. Die [VRM-Lizenz](https://vrm.dev/en/licenses/1.0/) arbeitet zusammen mit den [Lizenzangaben in den Modellmetadaten](https://github.com/vrm-c/vrm-specification/blob/master/specification/VRMC_vrm-1.0/meta.md): Avatar-Nutzung, kommerzielle Verwendung, Bearbeitung, Weitergabe und Namensnennung dürfen nicht allein aus dem Wort „Sample“ abgeleitet werden. Solche Beispiele sind hier keine pauschal freigegebenen Produktfiguren.

## Ready Player Me

Die aktuelle Verfügbarkeit eines Online-Creators oder einer Avatar-API konnte über die offiziellen [Hauptseiten](https://readyplayer.me/) und [Dokumentation](https://docs.readyplayer.me/) bei dieser Recherche nicht verifiziert werden; die Abrufe schlugen fehl. Ältere SDK- und Integrationsseiten belegen keinen aktuell funktionierenden Dienst. Deshalb wird Ready Player Me hier nicht als verfügbare Online-Integration zugesagt. Eine bereits vorhandene, rechtmäßig nutzbare GLB-Datei kann über den allgemeinen Dateiimport verwendet werden; ihre konkreten Rechte sind getrennt zu prüfen.

## Auswahl für dieses Projekt

Für zusätzliche mitgelieferte Figuren sind dokumentierte CC0-/Attributionsmodelle mit enthaltenen Animationen am einfachsten nachvollziehbar. Für eine eigene VTuber-Identität eignet sich ein selbst gestalteter VRM-Charakter. Der Importpfad bleibt dateibasiert, sodass diese Auswahl nicht vom Betrieb eines fremden Avatar-Dienstes abhängt.
