# Charaktere erstellen und finden

Recherche: **19. Oktober 2026**. Diese Auswahl richtet sich nach Dateiformat, Bearbeitbarkeit und Eignung für OrbitPal. Sie ist kein Beliebtheitsranking. Die verlinkten Seiten wurden recherchiert; damit ist kein pauschaler Funktionstest aller dort angebotenen Modelle verbunden.

Für einen eigenen Anime-Charakter ist **VRoid Studio** der einfachste Einstieg. Für einen menschlichen Avatar aus einem Foto kommt **Avaturn** infrage. Bei fertigen Figuren bieten **Open Source Avatars**, **VRoid Hub** und **BOOTH** viele Ansatzpunkte. Für stilisierte Monster, Tiere und Spielfiguren lohnen sich **Quaternius** und **KayKit**.

## Eigene Charaktere erstellen

| Werkzeug | Wofür es sich eignet | Weg nach OrbitPal |
| --- | --- | --- |
| [VRoid Studio](https://vroid.com/en/studio) | Anime- und VTuber-Figuren; Gesicht, Körper, Haare, Kleidung und Texturen anpassen. | Als **VRM** exportieren und die `.vrm`-Datei importieren. Projektdateien wie `.vroid` bleiben im Editor. |
| [Avaturn](https://avaturn.me/) | Realistischere menschliche Avatare aus einem Selfie mit anpassbarer Kleidung und Frisur. | Einen **GLB**-Export verwenden. Die offizielle [SDK-Dokumentation](https://docs.avaturn.me/docs/integration/web/html/) bestätigt GLB als Exportformat. Konto, Exportumfang und aktuelle Nutzungsbedingungen des Dienstes prüfen. |
| [Blender](https://www.blender.org/) | Eigene Roboter, Tiere, Monster und frei gestaltete Figuren; Modellierung, Texturen, Rig und Animationen. | **glTF Binary (`.glb`)** exportieren. Blender ist auch der Konvertierungsweg für passende Fremdformate. |
| [MakeHuman](https://static.makehumancommunity.org/) | Parametrisch erzeugte menschliche Körper als Ausgangspunkt. | In Blender weiterbearbeiten und als GLB exportieren. Die [MakeHuman-Exportdokumentation](https://static.makehumancommunity.org/makehuman/docs/exports_and_file_formats.html) beschreibt unter anderem FBX, DAE und OBJ; das bedeutet keinen direkten Import dieser Formate in OrbitPal. |
| [MPFB für Blender](https://static.makehumancommunity.org/mpfb.html) | Menschliche Figuren direkt in Blender erzeugen, mit Haut, Haaren, Kleidung und Rig ausstatten. | In Blender als GLB exportieren. MPFB benötigt keine zusätzliche MakeHuman-Installation; siehe [Einstieg](https://static.makehumancommunity.org/mpfb/docs/getting_started.html). |

VRoid Studio erlaubt für selbst erstellte Inhalte eigene Nutzungsbedingungen. Bei übernommenen Modellen, Kleidungsstücken oder Texturen gelten zusätzlich deren Bedingungen. Die [MPFB-Lizenzübersicht](https://static.makehumancommunity.org/mpfb/faq/is_it_really_free.html) nennt CC0 für die mitgelieferten Kernressourcen; zusätzliche Community-Assets können eigene Lizenzen haben.

## Bibliotheken mit fertigen Figuren

| Quelle | Auswahl und praktische Suche | Import und Bedingungen |
| --- | --- | --- |
| [Open Source Avatars – Finder](https://www.opensourceavatars.com/en/finder) | Auf Avatare ausgerichtete Sammlung mit verschiedenen Kollektionen. Die [Ressourcenseite](https://www.opensourceavatars.com/en/resources) ergänzt den dynamisch geladenen Finder. | Vorzugsweise **VRM** herunterladen. Der [Lizenzleitfaden](https://www.opensourceavatars.com/en/resources/about/license) unterscheidet CC0 und CC-BY: Bedingungen der jeweiligen Kollektion und Datei prüfen. |
| [VRoid Hub](https://hub.vroid.com/en) | Community für 3D-Charaktere, besonders Anime- und VTuber-Modelle. | Eine tatsächlich zum Download freigegebene **VRM-Datei** wählen. Sichtbarkeit im Viewer ist keine Download- oder Nutzungserlaubnis. Zugang und Nutzungsfreigaben hängen vom Angebot ab; eine Anmeldung kann nötig sein. |
| [BOOTH – 3D Characters mit VRM-Suche](https://booth.pm/en/browse/3D%20Characters?q=VRM) | Viele kostenlose und kostenpflichtige Creator-Angebote, darunter komplette Figuren und ungewöhnliche Maskottchen. | In der Produktbeschreibung ausdrücklich nach enthaltenen **`.vrm`-Dateien** suchen. Ein VRM-Tag kann auch Kleidung, Texturen oder Zubehör bezeichnen; beispielsweise ist [dieses VRoid-Kleidungsangebot](https://booth.pm/en/items/4075498) ein Custom Item. `.unitypackage` oder `.vroidcustomitem` sind keine direkt importierbaren Charakterdateien. |
| [Quaternius](https://quaternius.com/) | Stilisierte Low-Poly-Menschen, Monster, Tiere und Roboter; mehrere Pakete enthalten Animationen. | Das konkrete Paketformat prüfen. **GLB/glTF** bevorzugen; FBX oder Blender-Dateien vorher in Blender umwandeln. Die Seite zum [Ultimate Monsters Pack](https://quaternius.com/packs/ultimatemonsters.html) nennt animierte Figuren und CC0. Das ist keine Aussage über jedes andere Paket oder seine Zusatzinhalte. |
| [KayKit / Kay Lousberg](https://kaylousberg.itch.io/) | Stilisierte Spielfiguren, Skelette und weitere Charakterpakete mit passenden Animationen. | [Adventurers](https://kaylousberg.itch.io/kaykit-adventurers) nennt riggierte Figuren, Basisanimationen, FBX/glTF und CC0. Den glTF-Download einschließlich Texturen verwenden oder in Blender ein GLB erstellen. Kostenlose und zusätzliche kostenpflichtige Paketstufen unterscheiden. |
| [Sketchfab – herunterladbare Modelle](https://sketchfab.com/features/gltf) | Breite Auswahl von Menschen über Kreaturen bis zu Robotern; gezielt nach **downloadable**, **rigged** und **animated** suchen. | Nur freigegebene Downloads verwenden und das konkrete GLB/glTF prüfen. Nicht jedes Modell hat Rig, Mundformen oder Animationen. Die [Download-API-Dokumentation](https://sketchfab.com/developers/download-api) beschreibt GLB/glTF und die notwendige Nutzeranmeldung für API-Downloads. Daraus folgt keine automatische Sketchfab-Anbindung in OrbitPal. |

## Welches Dateiformat wählen?

Die folgenden Angaben beschreiben den aktuellen Entwicklungsstand. Ältere installierte Releases können einen kleineren Importumfang haben.

- **VRM 0.x und VRM 1.0 (`.vrm`)**: Für humanoide Avatare besonders passend, weil das Format neben dem Modell auch standardisierte Avatarinformationen mitbringen kann. Die [VRM-Dokumentation](https://vrm.dev/en/vrm/vrm_features/) erklärt den Umfang. Welche Mundformen, Ausdrücke und Bewegungen tatsächlich vorhanden sind, hängt vom Modell ab.
- **GLB 2.0 (`.glb`)**: Bevorzugter allgemeiner Austauschweg. Geometrie, Texturen, Rig und enthaltene Animationsclips sollten gemeinsam eingebettet sein. Eine Dateiendung allein garantiert diese Inhalte nicht.
- **glTF 2.0 JSON (`.gltf`) mit Begleitdateien**: Der neue Import verarbeitet lokale relative BIN-, PNG- und JPEG-Ressourcen und bündelt sie in ein eingebettetes GLB. Das Downloadpaket zuerst vollständig entpacken und seine Ordnerstruktur erhalten. Danach die `.gltf` auswählen; einzelne Texturen ohne Modelldatei reichen nicht. Externe Webadressen werden dabei nicht nachgeladen.
- **FBX, OBJ und `.blend`**: Aktuell kein direkter Import. In Blender öffnen beziehungsweise importieren und als eingebettetes GLB exportieren.
- **VRMA**: Eine separate VRM-Animationsdatei, kein Charaktermodell. Ein eigenständiger VRMA-Import ist derzeit nicht enthalten.

Die offizielle [Blender-Dokumentation zu glTF 2.0](https://docs.blender.org/manual/en/5.1/addons/import_export/scene_gltf2.html) beschreibt GLB, getrennte glTF-Ressourcen sowie den Export von Skinning, Shape Keys und Animationen. Beim Export passende Animationen und Materialien einschließen und PNG/JPEG-Texturen bevorzugen. Erweiterungen, Spezialshader und Kompressionsverfahren können zusätzliche Loader benötigen; ein einfaches GLB ohne solche Abhängigkeiten ist der bessere Ausgangspunkt.

Für einen sprechenden Cohost nach vorhandenen **Mund-Blendshapes, VRM-Mundformen oder einem geeigneten Kieferknochen** suchen. Für Idle, Laufen und Gesten sind enthaltene Animationsclips hilfreich. Ein statisches Mesh wird durch den Import nicht automatisch zu einer vollständig animierten Figur. Separat heruntergeladene Animationen werden auch nicht allein dadurch mit einem fremden Skelett verbunden; bei Bedarf vorher in Blender zusammenführen und passend exportieren.

## Nutzung im Stream und Weitergabe der Dateien

Für jede konkrete Figur die Modelllizenz und die Bedingungen aller übernommenen Bestandteile aufbewahren. Dazu gehören Texturen, Kleidung, Haare und gegebenenfalls Animationen. Besonders bei VRoid Hub legt der Autor getrennte Bedingungen für Avatar-Nutzung, kommerzielle Verwendung, Bearbeitung, Weitergabe und Namensnennung fest; siehe die [offizielle Erklärung der Nutzungsbedingungen](https://vroid.pixiv.help/hc/en-us/articles/360011648594-Can-other-users-use-my-characters-without-permission).

Ein erlaubter Einsatz im eigenen Stream ist keine pauschale Erlaubnis, die Modelldatei in einem öffentlichen Repository, einem App-Installer oder einem Profilpaket weiterzugeben. Für eine mitgelieferte OrbitPal-Galerie braucht das konkrete Modell entsprechende Weitergaberechte. Die MIT-Lizenz von OrbitPal ersetzt diese Asset-Lizenzen nicht.

Die praktische Empfehlung: **VRoid Studio + VRM** für eine eigene VTuber-Figur; **OSA + klar dokumentierte Modelllizenz** für fertige VRM-Avatare; **KayKit oder Quaternius + GLB/glTF** für stilisierte Spielcharaktere. **Blender/MPFB** ist der Weg für eigene Gestaltung und Modelle, die erst konvertiert oder mit Animationen ausgestattet werden müssen.
