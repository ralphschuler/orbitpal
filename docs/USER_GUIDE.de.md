# Streamer AI Cohost

Eine eigenständige Desktop-App für einen konfigurierbaren KI-Stream-Charakter. Tauri + Rust bilden die native Anwendung, Svelte das Regiepult und Three.js den echten 3D-Charakter. Die Oberfläche ist deutsch.

## Start

Der mitgelieferte native Build von `Streamer AI Cohost.app` ist für **macOS auf Apple Silicon** (M1 oder neuer, macOS 12+) erstellt. Er hat **keine Developer-ID-Signatur oder Notarisierung**. Falls macOS den Start blockiert, kann die App nach dem ersten Öffnungsversuch in **Systemeinstellungen → Datenschutz & Sicherheit → Dennoch öffnen** freigegeben werden. Weitere Plattformpakete werden über die GitHub-Releases bereitgestellt; der Status steht beim jeweiligen Release.

Die App benötigt zum Start weder ein kostenpflichtiges KI-Konto noch ein heruntergeladenes 3D-Modell. Nova ist als eigene prozedurale 3D-Figur enthalten. Für Dialoge muss ein lokales Modell, eine Cloud-API oder eine separat installierte und angemeldete Claude-Code-/Codex-CLI konfiguriert werden.

1. Unter **Modelle** die Integration wählen. Für API-Anbieter Basis-URL und Modell eintragen oder **Modelle laden** verwenden. Ollama und LM Studio müssen separat laufen. Für Claude Code oder Codex den installierten CLI-Befehl beziehungsweise seinen absoluten Programmpfad eintragen und die betreffende CLI vorher separat anmelden. Cloud-API-Schlüssel werden im Systemschlüsselbund gespeichert.
2. Unter **Charakter & Stimme** Name, Darstellung, Stimme und Emotes einstellen. Die Systemstimme ist eine Betriebssystemstimme. Eigene Kunststimmen benötigen eine gespeicherte Stimme beim TTS-Anbieter beziehungsweise ein lokales Stimmenmodell.
3. Im **Regiepult** direkt mit dem Charakter sprechen oder unter **Verbindungen** Twitch und YouTube einrichten.
4. **Manuell auswählen** beantwortet nur bewusst ausgewählte Nachrichten. Für automatische Antworten den Co-Host- oder Pausenmodus wählen und **Cohost starten** aktivieren.
5. Geschriebene Chatantworten können unter **Verbindungen** für Twitch und YouTube getrennt aktiviert werden. Sie sind standardmäßig aus. Die gesprochene Antwort funktioniert unabhängig davon.

## Enthalten

- Eigene animierte 3D-Roboterfigur Nova mit Blick, Blinzeln, Ruhebewegungen, Gesichtsausdrücken und sprachgesteuertem Mund.
- 17 mitgelieferte Charaktere: Nova, Robot Expressive, Fox, KayKit-Ritter, -Magier, -Schurke und -Skelett sowie zehn CC0-VRM-Figuren aus [Open Source Avatars](https://www.opensourceavatars.com/en/finder). Quellen, Modellgrenzen und weitere Importmöglichkeiten stehen in [CHARACTERS.md](../CHARACTERS.md); die [Attributionshinweise](../public/characters/ATTRIBUTION.md) und [OSA-Metadaten](../public/characters/osa/CREDITS.json) gehören zu den Fremdmodellen. Die KayKit-Figuren enthalten Körperanimationen; die zehn OSA-VRMs besitzen Humanoidrigs und Vokalmorphs und verwenden die prozeduralen Gesten der App.
- Import eigener VRM- und GLB-Modelle. Enthaltene GLB-Animationsclips können als eigene Emotes zugeordnet werden.
- Emote-Bibliothek pro Profil: Winken, Verstecken, Freuen sowie Nicken und Tanzen. Namen, Dauer, Intensität und Freigabe für Zuschauer-Anfragen sind konfigurierbar. Der Agent kann freigegebene Emotes als Werkzeug ausführen.
- Dialogmodelle über Ollama, LM Studio, OpenAI-kompatible APIs, Anthropic Messages sowie die installierten Claude-Code- und Codex-CLIs. Begrenzte Werkzeugrunden und Abbruch; eintreffende Modelltexte werden intern gesammelt und mit der Sprachausgabe veröffentlicht.
- Systemstimmen, ElevenLabs und kompatible lokale/Cloud-TTS-Endpunkte. Push-to-talk mit lokalem Whisper, ElevenLabs Scribe, OpenAI Whisper oder kompatiblen STT-Endpunkten. Aufnahmen werden lokal in Mono-PCM16-WAV mit 16 kHz umgewandelt.
- Twitch EventSub und YouTube-Livechat gleichzeitig, begrenzte Warteschlange und Verfallszeit für alte Nachrichten. Die erste YouTube-Anbindung verwendet `liveChatMessages.list` mit dem vom Server vorgegebenen Abrufintervall.
- Optionale geschriebene Antworten im jeweiligen Quellchat. Pro Antwort wird höchstens eine Nachricht gesendet; längere Texte werden auf 500 Zeichen bei Twitch beziehungsweise 200 Zeichen bei YouTube gekürzt. Die eigene Bot-Identität wird aus dem Eingang ausgefiltert.
- Eigene `AGENTS.md` und `PERSONA.md`, direkt im Editor oder als Datei bearbeitbar.
- Agent Skills mit `SKILL.md`, Referenzen, Assets und optionalen Node-/Python-Skripten. Skill-Ordner können importiert werden. Skriptausführung ist gesondert schaltbar und auf Betreiber-Anfragen beschränkt.
- MCP-Server lokal über stdio und entfernt über Streamable HTTP. Werkzeuge, Ressourcen und Prompts werden mit Freigaben pro Charakter angeboten. Unterstützung moderner und älterer MCP-Protokolle.
- SQLite-Gesprächsverlauf, begrenzter Modellkontext, Wissensdateien, portable Profile und separate Zugangsdaten im Systemschlüsselbund.
- Transparentes, rahmenloses Figurenfenster, optional immer im Vordergrund und mit durchgereichten Mausklicks. Separates Regiepult, Traymenü und OBS-Browserquelle.

## Mehrere Bots und Stream-Crew

Über die Bot-Auswahl in der linken Seitenleiste oder **Neuen Bot anlegen** einen weiteren Charakter erstellen. Der Dialog enthält Name, mitgelieferte Figur und die Option **Einstellungen übernehmen**. Im Regiepult verwaltet **Deine Stream-Crew** die Figurenfenster und den eigenen OBS-Link jedes Bots.

Mehrere Bots können **gleichzeitig sichtbar im Stream** sein. Jeder hat seine eigene Persönlichkeit, Stimme, Modellwahl, Ansprache, Emotes sowie MCP-/Skill-Freigaben. Die Auswahl in der Seitenleiste bestimmt, welches Profil im Regiepult bearbeitet wird; ein Wechsel schließt die anderen Figurenfenster nicht. Jedes Fenster behält seine Position und erhält nur die Profil-, Sprach- und Emote-Ereignisse seines Bots. OBS-Links enthalten neben der sitzungsgebundenen Kennung eine eigene Bot-ID.

| Beim Anlegen | Ergebnis |
| --- | --- |
| **Frisch beginnen** | Eigenes Profil mit Vorgaben, eigenen `AGENTS.md`-/`PERSONA.md`-Dateien, Skill-/Wissensordnern, leerem Verlauf und neuen Schlüsselbund-Referenzen. |
| **Einstellungen übernehmen** | Modell-, Stimmen- und weitere Einstellungen sowie Anweisungen, Skills und Wissen werden in ein eigenständiges Profil kopiert. Der Name in `PERSONA.md` wird angepasst; der Gesprächsverlauf bleibt leer. Vorhandene Schlüsselbund-Referenzen werden weiterverwendet, geheime Werte werden nicht kopiert. MCP-Server erhalten neue interne IDs. |

Die App speichert den Verlauf pro Bot. Änderungen an den Anweisungs- und Skill-Dateien eines Bots verändern die Kopie eines anderen Bots nicht. Bei übernommenen Einstellungen können beide Bots dieselbe im Schlüsselbund gespeicherte Anbieteranmeldung nutzen; dadurch teilen sie gegebenenfalls auch dessen Kontingent.

Twitch und YouTube bilden die gemeinsame Live-Chat-Eingabe der Crew. Gleichzeitig wird jeweils eine Verbindung pro Plattform gehalten; die Auswahl eines anderen Botprofils trennt diese Verbindungen nicht. Die Bots wenden ihre eigenen Auslöser und Antwortmodi an. Gespräche und Sprachausgaben werden **nacheinander** verarbeitet, während alle eingeblendeten Figuren sichtbar bleiben. **Ausgabe beenden** betrifft den gewählten Bot; der zentrale **Stop** bricht ausstehende Antworten und Sprache aller Ausgaben ab. Dialog-, STT- und TTS-Integration werden pro Profil gewählt; eine CLI-Anmeldung und Cloud-API-Schlüssel sind weiterhin getrennte Zugänge.

## Dialoganbieter und CLI-Anbindung

Unter **Modelle** kann zwischen API-Anbindungen und einer installierten **Claude Code CLI** oder **Codex CLI** gewählt werden. Beide Programme müssen separat installiert sein. Die App startet den eingetragenen Befehl als lokalen Prozess; ein absoluter Pfad zur ausführbaren Datei vermeidet Unterschiede zwischen dem Terminal-PATH und dem PATH einer Desktop-App. Ohne eigenen Programmpfad verwendet sie `claude` beziehungsweise `codex`. Der Modellname ist optional: Ein leeres Feld oder `default` verwendet die Vorgabe der CLI. Ein ausdrücklich eingetragener Modellname wird unverändert übergeben.

Benötigt werden **Codex CLI ab 0.159.3** beziehungsweise **Claude Code ab 2.1.248**. Über **CLI prüfen** lässt sich die installierte Version ohne Modellaufruf abfragen.

Die Installation und der Prozess einer CLI sind lokal. Die Modellberechnung erfolgt bei der üblichen Claude-Code-/Codex-Konfiguration beim Cloud-Anbieter. Die CLI-Auswahl verspricht deshalb weder Offline-Betrieb noch lokale Modellinferenz. Für ein tatsächlich lokales Dialogmodell Ollama, LM Studio oder einen entsprechenden lokalen API-Endpunkt verwenden.

Die Anmeldung erfolgt außerhalb des Cohosts mit der jeweiligen CLI:

```sh
claude auth login
```

```sh
codex login
```

Bei einem absoluten Programmpfad denselben installierten Befehl auch für die Anmeldung verwenden. Die CLIs verwalten ihre eigenen Sitzungen und Zugangsdaten. **Im Cohost gespeicherte API-Schlüssel werden nicht automatisch in die CLI übernommen.** Verfügbare Modelle, Kontingente und Abrechnung richten sich nach der tatsächlichen CLI-Anmeldung und dem Anbieterzugang. Die offiziellen Anleitungen beschreiben die [Claude-Code-Anmeldung](https://code.claude.com/docs/en/authentication) und die [Codex-Anmeldung](https://learn.chatgpt.com/docs/auth).

Charakter, `AGENTS.md`, `PERSONA.md`, Gesprächskontext und Cohost-Werkzeuge werden weiterhin durch unsere Agenten-Laufzeit zusammengestellt. CLI-Anbieter verwenden dafür eine strukturierte Host-Tool-Bridge: Das Modell kann Werkzeugaufrufe anfordern, der Cohost führt freigegebene MCP-Werkzeuge, Skills, Wissenssuche und Emotes aus und liefert die Ergebnisse für die nächste Antwort zurück. Die bestehenden Betreiber-/Chat-Freigaben bleiben wirksam. Diese Anbindung ist keine automatische Übernahme aller separat in der CLI installierten Erweiterungen. Die Codex-Integration verwendet `app-server`; die Claude-Code-Integration den Print-Modus. Die CLI-eigenen Betriebssystemwerkzeuge werden für diese Dialogaufrufe eingeschränkt, während der Cohost die freigegebenen Aktionen ausführt.

## Mikrofon und Spracherkennung

Im Bereich **Modelle** die Spracherkennung unabhängig vom Dialogmodell und von der Stimme auswählen. Die Mikrofontaste startet eine begrenzte Aufnahme; beim Beenden wird sie transkribiert und der erkannte Text als Betreiber-Nachricht an den Cohost übergeben. Sprache `de` ist die Vorgabe; `auto` oder ein leeres Sprachfeld aktiviert die automatische Spracherkennung. Diese Version verarbeitet abgeschlossene Aufnahmen; eine dauerhafte Realtime-Transkription ist nicht enthalten.

| Auswahl | Voraussetzung und Standardkonfiguration |
| --- | --- |
| **Whisper lokal · whisper.cpp** | Separat laufender `whisper-server` und ein heruntergeladenes Whisper-Modell. Basis-URL `http://127.0.0.1:8080`, Anfrage an `/inference`. Das Modell wird im Server geladen; der Cohost lädt keine Gewichte herunter. |
| **Whisper lokal · kompatible API** | Separater lokaler STT-Server mit OpenAI-kompatiblem `/audio/transcriptions`-Endpunkt. Basis-URL mit `/v1` sowie den vom Server unterstützten Modellnamen eintragen. |
| **OpenAI Whisper · Cloud** | Eigener OpenAI-API-Schlüssel, Basis-URL `https://api.openai.com/v1`, Modell `whisper-1`. Anfrage an `/audio/transcriptions` mit Bearer-Authentifizierung. |
| **ElevenLabs Scribe · Cloud** | Eigener ElevenLabs-API-Schlüssel, Basis-URL `https://api.elevenlabs.io/v1`, Modell `scribe_v2`. Anfrage an `/speech-to-text` mit `xi-api-key`. |
| **Kompatible API** | Eigener lokaler oder entfernter Multipart-Endpunkt nach OpenAI-Format, konfigurierbare Basis-URL und Modell. Bei Bedarf separaten Schlüssel speichern. |

Die Auswahl **Whisper lokal** akzeptiert ausschließlich Loopback-Adressen und leitet Anfragen nicht über einen Proxy oder HTTP-Redirect weiter. Für externe kompatible Server die allgemeine Auswahl **Kompatible API** verwenden. Bei Cloud-STT wird die abgeschlossene Aufnahme an den gewählten Anbieter übertragen; ein lokal gewähltes Dialogmodell ändert daran nichts.

Für whisper.cpp beispielsweise einen vorhandenen Server mit eigenem Modell starten; die Pfade an die Installation anpassen:

```sh
/absoluter/pfad/whisper-server -m /absoluter/pfad/ggml-base.bin --host 127.0.0.1 --port 8080
```

Der Server und die Modellgewichte sind nicht im App-Paket enthalten. Die App dekodiert ihr Aufnahmeformat im WebView und erzeugt vor dem Versand **Mono-WAV, 16 kHz, PCM16**. Für diese Standardaufnahme sind weder `ffmpeg` noch der whisper.cpp-Schalter `--convert` erforderlich. Kann der WebView ein Aufnahmeformat nicht dekodieren, wird ein Fehler angezeigt. Eingabe und erzeugte WAV-Datei sind auf jeweils 25 MiB begrenzt; Anbieter können zusätzliche oder kleinere Grenzen anwenden. Siehe [whisper.cpp-Server](https://github.com/ggml-org/whisper.cpp/tree/master/examples/server), [OpenAI-Dateitranskription](https://developers.openai.com/api/docs/guides/speech-to-text) und [ElevenLabs-Transkription](https://elevenlabs.io/docs/api-reference/speech-to-text/convert).

## Twitch und YouTube

### Twitch

Du benötigst eine eigene Twitch-Client-ID und ein dazu passendes **User OAuth Token** mit `user:read:chat`. Broadcaster-ID bezeichnet den Kanal. Die Bot-Nutzer-ID wird aus dem Token ermittelt, wenn sie nicht eingetragen ist. Twitch verlangt je nach Bot-/Kanal-Konstellation weitere Berechtigungen beziehungsweise Autorisierung des Broadcasters. Für optionale Chatantworten wird zusätzlich `user:write:chat` benötigt.

Die App erstellt eine EventSub-WebSocket-Verbindung und zeigt den Status erst nach erfolgreicher Autorisierung und Subscription als verbunden an. Token-Erstellung und Anmeldung erfolgen bei Twitch; es werden keine fremden Konten oder Tokens mitgeliefert. Für geschriebene Antworten muss **Antworten auch in den Chat senden** aktiviert werden (`chat.twitch.sendReplies`). Dafür wird derselbe gespeicherte Token verwendet; die zusätzliche Berechtigung `user:write:chat` muss bereits bei seiner Erstellung gewährt worden sein.

### YouTube

Aktiviere die YouTube Data API in deinem Google-Projekt und trage einen API-Schlüssel sowie die URL/ID eines aktuell laufenden Live-Videos ein. Die aktive Live-Chat-ID wird daraus ermittelt; alternativ kann sie direkt angegeben werden. Abrufe verbrauchen API-Kontingent.

Für geschriebene Antworten **Antworten auch in den Chat senden** aktivieren (`chat.youtube.sendReplies`) und im separaten Feld einen **OAuth-Zugriffstoken** speichern. Er benötigt den Scope `https://www.googleapis.com/auth/youtube.force-ssl` oder `https://www.googleapis.com/auth/youtube`. Der Token wird standardmäßig unter der Schlüsselbund-ID **`youtube-write`** gespeichert (`chat.youtube.sendSecretId`); der Lese-API-Schlüssel bleibt separat unter `youtube`. Die App ermittelt den Kanal des Schreibtokens vor dem ersten Senden, um eigene Nachrichten nicht erneut zu beantworten. Token-Erstellung und Erneuerung erfolgen außerhalb der App. Der API-Schlüssel allein kann keine Nachrichten schreiben.

Geschriebene Antworten benötigen eine aktive Chatverbindung. Ein Sendefehler wird als Aktivität angezeigt und entfernt die gesprochene Antwort nicht.

## Darstellung

- **Studio:** Figur und Sprache laufen im Regiepult.
- **Desktop-Overlay:** Die Figur läuft in einem eigenen rahmenlosen Fenster. Zum Verschieben die beim Überfahren eingeblendete Griffleiste verwenden. Im Regiepult kann das Durchreichen von Mausklicks wieder deaktiviert werden.
- **OBS:** **OBS-Link** kopieren und in OBS eine Browserquelle mit transparentem Hintergrund anlegen. Der Link enthält eine sitzungsgebundene Zugriffskennung. Bei einem Neustart der App muss er erneut kopiert werden. Für reinen Streambetrieb OBS auswählen, um doppelte Sprachausgabe zu vermeiden.

Antworttext, Gesichtsausdruck und vom Agenten eingeplante Emotes beginnen mit der tatsächlichen Audiowiedergabe. Bei API-TTS muss die Audiodatei vollständig vorliegen und dekodiert sein. Optionale Twitch-/YouTube-Chatantworten werden einmalig beim selben Startsignal verschickt; ihre Ankunft hängt zusätzlich vom jeweiligen Dienst ab. Bei ausgeschalteter Sprachausgabe erscheinen Text und Emotes sofort. Manuell ausgelöste Emotes bleiben direkt bedienbar. Stop und Audiofehler verwerfen ausstehende Ausgabeaktionen.

Eine aktive Audioausgabe steuert die Mundbewegung. Bei API-TTS wird die tatsächliche Audioamplitude vor dem Lautstärkeregler gemessen. Vorhandene Vokalmorphs beziehungsweise Kieferknochen öffnen und schließen den Mund, auch bei leiser Sprache; Figuren ohne Mundrig verwenden Sprechgestik. Die wechselnden Mundformen sind eine visuelle Näherung und keine Phonemerkennung. Systemstimmen stellen keine Roh-Audiodaten bereit; ihre Animation verwendet Sprachstart, Wortgrenzen und Sprachende. Der komplette Untertitel wird angezeigt; wortgenaue Hervorhebung ist nicht implementiert.

Unter Linux hängen freie Fensterpositionierung, Transparenz und „immer im Vordergrund“ vom Desktop, X11/Wayland und Compositor ab. Der Browser-Renderer ist eine zusätzliche Stream-Ausgabe unabhängig vom Desktopfenster.

## MCP und Skills

Im Bereich **Erweiterungen** Server hinzufügen, verbinden und anschließend die angebotenen Werkzeuge freigeben. Die Spalte **Cohost** erlaubt die Nutzung für Betreiber-Anfragen; **Chat** erlaubt zusätzlich Zuschauer-Anfragen. Leere Freigaben bedeuten keinen Zugriff. Das Dialogmodell muss zur Integration passendes Tool Calling unterstützen.

Lokale MCP-Server benötigen ihre eigenen Laufzeiten. Der Startbefehl wird direkt mit Argumenten aufgerufen, nicht über eine Shell. Für aufwendigere Startumgebungen können absolute Programmpfade verwendet werden. In dateibasierter MCP-Konfiguration können Umgebungswerte auf Schlüsselbund-Einträge verweisen: `${secret:meine-id}`. Profilexporte enthalten keine literalen Umgebungs- oder Header-Geheimnisse.

Unter Windows bevorzugt die App native `.exe`-Programme. Bekannte npm-Startdateien `codex.cmd`, `claude.cmd`, `npm.cmd` und `npx.cmd` werden ohne Batch-Shell über `node.exe` und die zugehörige installierte JavaScript-Datei gestartet. Node.js und die betreffende CLI beziehungsweise der MCP-Server müssen separat installiert sein. Bei anderen Batchwrappern einen echten `.exe`-Pfad verwenden. Für einen manuell eingerichteten npm-MCP-Start kann beispielsweise `C:\Program Files\nodejs\node.exe` als Programm und `C:\Program Files\nodejs\node_modules\npm\bin\npx-cli.js` als erstes Argument eingetragen werden; die Serverargumente folgen als weitere Listeneinträge. Es wird keine zusammengesetzte Shell-Befehlszeile ausgeführt.

`SKILL.md` wird bei Bedarf geladen. `scripts/`, `references/` und `assets/` werden mit importiert. Skripte sind standardmäßig ausgeschaltet und benötigen bei Aktivierung eine separat verfügbare Node-/Python-Laufzeit. Ein Beispielprofil liegt unter `character-template/`.

## Wissen und Hindsight

Im Bereich **Gedächtnis** PDF, TXT, Markdown, JSON oder CSV importieren. PDF-Dateien sind auf 20 MiB und 500 Seiten begrenzt, extrahierter Text und UTF-8-Dateien auf 4 MiB; pro Bot maximal 128 Dokumente und 16 MiB indizierter Text. Die App erhält die Originaldatei und indexiert den Text beim Import. Scans ohne Text benötigen vorher OCR. Gleichnamige Dateien werden nicht überschrieben.

Vor jeder Antwort sucht die App lokal nach passenden Stichwörtern und fügt begrenzte Auszüge automatisch ein, auch bei ausgeschalteter Werkzeugnutzung. Quellen tragen Dateiname, Textzeilen und bei PDFs eine Seitennummer. PDF-Zeilen beziehen sich auf den extrahierten Text. Das Modell wird angewiesen, beim Bezug darauf die Quelle zu nennen; die tatsächliche Befolgung hängt vom Modell ab. Die zusätzliche Wissenssuche als Werkzeug ermöglicht weitere Nachfragen. Bei einem Cloud-Dialogmodell gehen relevante Auszüge an dessen Anbieter. Wissen bleibt getrennt vom Langzeitgedächtnis und wird mit dem Profil exportiert.

**Hindsight** ist das einzige Langzeitgedächtnis. Es ist standardmäßig aus. Eigene Server-URL und optional API-Schlüssel eintragen, automatische Erinnerungssuche und Speicherung getrennt wählen und speichern. Ohne Bank-ID wird die Bot-ID verwendet. Neue Bots erhalten eigene Banken; dieselbe Bank-ID teilt Erinnerungen absichtlich. Importierte Profile müssen Hindsight erneut aktivieren.

Der Recall ergänzt relevante Erinnerungen vor der Antwort; bei Fehlern läuft das Gespräch mit einer sichtbaren Aktivitätsmeldung weiter. Neue Nutzer-Nachrichten und Antworten werden erst beim tatsächlichen Ausgabestart einmalig zum Speichern eingereicht. Eine angenommene asynchrone Speicheroperation bedeutet noch nicht, dass Hindsight sie fertig verarbeitet hat. Es werden keine alten Verläufe, Wissensdateien oder Profile gesammelt hochgeladen. Erinnerungen werden auf dem Hindsight-Server verwaltet und gelöscht. Auch ein lokaler Hindsight-Server kann konfigurierte Cloud-Modelle und Embeddings verwenden. Der lokale SQLite-Verlauf bleibt als begrenzter Kurzzeitkontext erhalten; **Verlauf leeren** löscht keine Hindsight-Erinnerungen.

## Aktivierungsphrase am Mikrofon

Die Mikrofon-Statuszeile enthält aufklappbare Einstellungen für eigene Phrasen, etwa **hey nova**. Die Erkennung startet nur durch bewusstes Einschalten des Mikrofons. Nach der Phrase wird der folgende Text als Nachricht verarbeitet; bei einer Phrase ohne weiteren Text öffnet sich ein kurzes Nachfragefenster. Während laufender Antwort, Sprachausgabe oder manueller Aufnahme pausiert die Erfassung. Stop und Botwechsel beenden sie.

Die Funktion erkennt Phrasen in den Transkripten kurzer VAD-Aufnahmen. Sie ist kein separates Offline-Wake-Word-Modell. Bei Cloud-STT werden Audioabschnitte schon vor Erkennung der Phrase an den ausgewählten Anbieter geschickt. Für lokale Verarbeitung einen lokalen Whisper-Server verwenden. Gespeicherte Aktivierung startet nach einem App-Neustart kein Mikrofon automatisch.

## Antworttempo und CLI-Modelle

Unter **Modelle** stehen für Codex **Modelle & Denkstufen laden** und für Claude Code bekannte Modellvorgaben plus eigene Modell-ID bereit. Das Codex-Katalogergebnis kann aus der CLI-Konfiguration beziehungsweise einem Cache stammen und ist keine Zusage für Kontozugriff. Denkstufen werden passend zum ausgewählten Modell angeboten; **CLI-Vorgabe** überschreibt keine Vorgabe der CLI. Die Schnellvorgabe verwendet bei Claude Haiku, bei Codex eine niedrige angebotene Denkstufe sowie kurze Antworten, eine Werkzeugrunde und zwölf Kontextnachrichten. Änderungen speichern. Modellgeschwindigkeit, Werkzeug- und TTS-Laufzeit bleiben anbieterabhängig.

## Entwicklung

Siehe [CONTRIBUTING.md](../CONTRIBUTING.md) für Installation, lokale Tests und Builds, [ARCHITECTURE.md](ARCHITECTURE.md) für die Komponenten und [RELEASING.md](RELEASING.md) für automatische Pakete.

## Daten und Grenzen dieser Version

Die App legt Charakterdateien, Wissensdateien und die SQLite-Datenbank in ihrem Betriebssystem-App-Datenordner ab. Der Ordner ist in der Oberfläche zugänglich. API-Schlüssel und Tokens liegen separat im Betriebssystem-Schlüsselbund. Profile werden als JSON-Bundle einschließlich der verwalteten Charakter-/Skillressourcen exportiert.

Cloud- und lokale Stimmen sind getrennte Ressourcen. Eine ElevenLabs-Stimme wird nicht als lokales Stimmenmodell heruntergeladen. Benutzer stellen eigene Anbieterzugänge, lokale Modelllaufzeiten, gegebenenfalls CLI-Installationen mit eigener Anmeldung, STT-Server und Whisper-Modelle, Stimmen und eigene 3D-Dateien bereit. CLI-Anmeldung und Zugriff auf Cloud-STT sind unabhängig; ein Codex- oder Claude-Code-Login ersetzt keinen OpenAI-/ElevenLabs-STT-API-Schlüssel.

Eigenes Modelltraining, automatische Token-Erstellung bei Streaming-Plattformen und die Erzeugung eines individuellen 3D-Modells aus Fotos sind nicht Bestandteil dieser Version. Angepasste Modelle können über die jeweiligen Modelllaufzeiten eingebunden werden. Automatisierte Tests verwenden lokale Fixtures und Mocks; sie ersetzen keinen Live-Test mit dem eigenen Stream-Setup. Streaming-Konten, Modellanbieter, TTS/STT-Dienste und fremde MCP-Server wurden nicht live verifiziert. Der native Build belegt keine gemessene Leistung oder erfolgreiche Live-Anbieteranbindung.

## Technische Quellen

- [Tauri](https://v2.tauri.app/)
- [MCP-Architektur](https://modelcontextprotocol.io/docs/learn/architecture)
- [Agent Skills](https://agentskills.io/specification)
- [Twitch EventSub](https://dev.twitch.tv/docs/eventsub/handling-websocket-events/)
- [Twitch Chatnachrichten senden](https://dev.twitch.tv/docs/api/reference/#send-chat-message)
- [YouTube Live Chat lesen](https://developers.google.com/youtube/data/live/reference/rest/v3/liveChatMessages/list)
- [YouTube Live Chat schreiben](https://developers.google.com/youtube/data/live/reference/rest/v3/liveChatMessages/insert)
- [three-vrm](https://github.com/pixiv/three-vrm)
- [Claude Code CLI](https://code.claude.com/docs/en/cli-reference)
- [Claude Code Anmeldung](https://code.claude.com/docs/en/authentication)
- [Codex App Server](https://learn.chatgpt.com/docs/app-server)
- [Codex Anmeldung](https://learn.chatgpt.com/docs/auth)
- [whisper.cpp-Server und WAV-Endpunkt](https://github.com/ggml-org/whisper.cpp/tree/master/examples/server)
- [OpenAI Whisper-Modell](https://developers.openai.com/api/docs/models/whisper-1)
- [OpenAI-Dateitranskription](https://developers.openai.com/api/docs/guides/speech-to-text)
- [ElevenLabs Speech to Text](https://elevenlabs.io/docs/api-reference/speech-to-text/convert)
