# OrbitPal

Eine eigenständige Desktop-App für einen konfigurierbaren KI-Stream-Charakter. Tauri + Rust bilden die native Anwendung, Svelte das Regiepult und Three.js den echten 3D-Charakter. Die Oberfläche unterstützt Deutsch, Englisch, Französisch und Spanisch. Diese Anleitung verwendet die deutschen Bezeichnungen.

Diese Anleitung beschreibt **OrbitPal 0.8.0** (zuvor Streamer AI Cohost). Für veröffentlichte Pakete gelten zusätzlich die Hinweise des jeweiligen [Releases](https://github.com/ralphschuler/orbitpal/releases/latest).

Unter **Updates & Version** kannst du automatisch oder manuell nach neuen GitHub Releases suchen. Download und Installation beginnen erst mit **Bestätigen & installieren**. Speichere vorher Entwürfe und beende laufende Ausgaben und das Mikrofon. Details, unterstützte Paketformate und die einmalige manuelle Aktualisierung älterer Versionen stehen unter [Updates](UPDATING.md).

## Sprache der Oberfläche

Oben im Studio zwischen **Deutsch**, **English**, **Français** und **Español** wählen. Die Auswahl wirkt sofort und bleibt auf diesem Rechner gespeichert. Beim ersten Start wird eine passende System-/Browsersprache gewählt, andernfalls Englisch. Gespräch, Stimme, Spracherkennung, Persönlichkeit, Wissen und eigene Emotenamen bleiben davon unabhängig. Details stehen unter [Sprachen der Oberfläche](LOCALIZATION.md).

## Start

Pakete und Prüfsummen stehen im [aktuellen Release](https://github.com/ralphschuler/orbitpal/releases/latest). Die Build-Pipeline sieht Windows x64, macOS Apple Silicon/Intel und Linux x64 vor; verfügbare Pakete stehen beim jeweiligen Release. Windows-Pakete sind unsigniert, macOS-Apps ad-hoc signiert und ohne Developer-ID oder Notarisierung. Falls macOS den Start blockiert, kann die App nach dem ersten Öffnungsversuch in **Systemeinstellungen → Datenschutz & Sicherheit → Dennoch öffnen** freigegeben werden. Hinweise zu Plattformvoraussetzungen und Prüfsummen stehen unter [RELEASING.md](RELEASING.md).

Die App benötigt zum Start weder ein kostenpflichtiges KI-Konto noch ein heruntergeladenes 3D-Modell. Nova ist als eigene prozedurale 3D-Figur enthalten. Für Dialoge muss ein lokales Modell, eine Cloud-API oder eine separat installierte und angemeldete Claude-Code-/Codex-CLI konfiguriert werden.

1. Unter **Modelle** die Integration wählen. Für API-Anbieter Basis-URL und Modell eintragen oder **Modelle laden** verwenden. Ollama und LM Studio müssen separat laufen. Für Claude Code oder Codex den installierten CLI-Befehl beziehungsweise seinen absoluten Programmpfad eintragen und die betreffende CLI vorher separat anmelden. Cloud-API-Schlüssel werden im Systemschlüsselbund gespeichert.
2. Unter **Charakter & Stimme** Name, Darstellung, Stimme und Emotes einstellen. Die Systemstimme ist eine Betriebssystemstimme. Eigene Kunststimmen benötigen eine gespeicherte Stimme beim TTS-Anbieter beziehungsweise ein lokales Stimmenmodell.
3. Im **Regiepult** direkt mit dem Charakter sprechen oder unter **Twitch & YouTube** die Livechats einrichten.
4. **Manuell auswählen** beantwortet nur bewusst ausgewählte Nachrichten. Für automatische Antworten den Co-Host- oder Pausenmodus wählen und **Cohost starten** aktivieren.
5. Geschriebene Chatantworten können unter **Twitch & YouTube** getrennt aktiviert werden. Sie sind standardmäßig aus. Die gesprochene Antwort funktioniert unabhängig davon.

## Selbstständiger Autopilot

Unter **Autopilot** kann jeder Bot unabhängig herumlaufen, kleine Gesten zeigen und nach einer ruhigen Phase selbst einen kurzen Beitrag beginnen. Bewegung funktioniert ohne Modellanfrage; die Sprachversuche haben ein rollendes Stundenbudget. Optional wird vor einem Beitrag ein fest gewähltes MCP-Beobachtungswerkzeug aufgerufen und dessen Bild dem bildfähigen Dialogmodell übergeben. Start gilt nur für die aktuelle Sitzung; **Stop** beendet alle Autopiloten. Einrichtung, Kosten und Grenzen stehen unter [AUTOPILOT.md](AUTOPILOT.md).

## Bedienung und Tastatur

Die kleinen Informationsschaltflächen erklären Einstellungen beim Überfahren und bei Tastaturfokus. Mit **Tab** und **Umschalt+Tab** sind sie ebenso wie die übrigen Bedienelemente erreichbar.

| Kürzel | Aktion |
| --- | --- |
| **?** oder **F1** | Tastaturhilfe öffnen; `?` nur außerhalb von Eingabefeldern. |
| **Strg/⌘ + S** | Aktuelle Einstellungen und Anweisungsänderungen speichern; im Erweiterungseditor dessen Inhalt speichern. |
| **Strg/⌘ + K** | Zum Regiepult wechseln und das Nachrichtenfeld fokussieren. |
| **Strg/⌘ + Enter** | Nachricht im Regiepult senden, wenn keine Aufnahme oder Antwort läuft. |
| **Alt/⌥ + 1–8** | Bereiche in der Reihenfolge der Seitenleiste wählen; nicht während der Texteingabe. |
| **Esc** | Zuerst sichtbare Hilfe oder einen Dialog schließen; sonst die laufende Ausgabe stoppen. |

Die acht Bereiche sind **Regiepult**, **Charakter & Stimme**, **Modelle**, **Persönlichkeit & Regeln**, **Wissen & Gedächtnis**, **Twitch & YouTube**, **Autopilot** und **Skills & MCP**. Botfreigaben in der Erweiterungsbibliothek werden unmittelbar gespeichert; andere Profiländerungen über **Änderungen speichern** sichern.

## Enthalten

- Eigene animierte 3D-Roboterfigur Nova mit Blick, Blinzeln, Ruhebewegungen, Gesichtsausdrücken und sprachgesteuertem Mund.
- 17 mitgelieferte Charaktere: Nova, Robot Expressive, Fox, KayKit-Ritter, -Magier, -Schurke und -Skelett sowie zehn CC0-VRM-Figuren aus [Open Source Avatars](https://www.opensourceavatars.com/en/finder). Quellen, Modellgrenzen und weitere Importmöglichkeiten stehen in [CHARACTERS.md](../CHARACTERS.md); [Charaktere erstellen und finden](CHARACTER-SOURCES.md) nennt weitere Editoren und Bibliotheken; die [Attributionshinweise](../public/characters/ATTRIBUTION.md) und [OSA-Metadaten](../public/characters/osa/CREDITS.json) gehören zu den Fremdmodellen. Die KayKit-Figuren enthalten Körperanimationen; die zehn OSA-VRMs besitzen Humanoidrigs und Vokalmorphs und verwenden die prozeduralen Gesten der App.
- Import eigener VRM-, GLB- und glTF-2.0-Modelle. glTF-Pakete vollständig entpacken und relative BIN-Dateien sowie PNG/JPEG-Texturen im Modellordner belassen; der Import bündelt sie zu einem portablen GLB. Tatsächlich vorhandene Animationsclips stehen unter **Was deine Figur mitbringt** zur stummen Vorschau und Übernahme als eigenes Emote bereit. Die [Formatübersicht](AVATAR-SUPPORT.md) beschreibt die automatische VRM-0/1-/GLB-2-Erkennung und den glTF-JSON-Import und klar gemeldete Importgrenzen.

- Emote-Bibliothek pro Profil: Winken, Verstecken, Freuen sowie Nicken und Tanzen. Namen, Dauer, Intensität, optionaler Sound mit Lautstärke und Freigabe für Zuschauer-Anfragen sind konfigurierbar. Der Agent kann freigegebene Emotes als Werkzeug ausführen.
- Dialogmodelle über Ollama, LM Studio, OpenAI-kompatible APIs, Anthropic Messages sowie die installierten Claude-Code- und Codex-CLIs. Begrenzte Werkzeugrunden und Abbruch; eintreffende Modelltexte werden intern gesammelt und mit der Sprachausgabe veröffentlicht.
- Systemstimmen, ElevenLabs und kompatible lokale/Cloud-TTS-Endpunkte. Push-to-talk mit lokalem Whisper, ElevenLabs Scribe, OpenAI Whisper oder kompatiblen STT-Endpunkten. Aufnahmen werden lokal in Mono-PCM16-WAV mit 16 kHz umgewandelt.
- Twitch EventSub und YouTube-Livechat gleichzeitig, begrenzte Warteschlange und Verfallszeit für alte Nachrichten. Die erste YouTube-Anbindung verwendet `liveChatMessages.list` mit dem vom Server vorgegebenen Abrufintervall.
- Optionale geschriebene Antworten im jeweiligen Quellchat. Pro Antwort wird höchstens eine Nachricht gesendet; längere Texte werden auf 500 Zeichen bei Twitch beziehungsweise 200 Zeichen bei YouTube gekürzt. Die eigene Bot-Identität wird aus dem Eingang ausgefiltert.
- Eigene `AGENTS.md` und `PERSONA.md`, direkt im Editor oder als Datei bearbeitbar.
- Gemeinsame Skill-/MCP-Bibliothek für alle Bots, mit unabhängigen Bot- und Zuschauerfreigaben. Skill-Ordner enthalten `SKILL.md`, Referenzen, Assets und optionale Node-/Python-Skripte. Skriptausführung ist zusätzlich pro Bot schaltbar und auf ausdrückliche Betreiber-Aufträge beschränkt.
- MCP-Server lokal über stdio und entfernt über Streamable HTTP. Echte Bildinhalte aus erlaubten MCP-Werkzeugen können an ein Vision-Modell weitergegeben werden; dafür gibt es eine eigene Einstellung pro Bot.
- SQLite-Gesprächsverlauf, begrenzter Modellkontext, Wissensdateien, portable Profile und separate Zugangsdaten im Systemschlüsselbund.
- Transparentes, rahmenloses Figurenfenster, optional immer im Vordergrund und mit durchgereichten Mausklicks. Separates Regiepult, Traymenü und OBS-Browserquelle.

## Haltung und Modellvarianten

Die Ruhehaltung und Gesten von VRM-Figuren werden automatisch aus dem jeweiligen Skelett abgeleitet, einschließlich der zehn älteren OSA-Modelle. Eine manuelle Armkorrektur ist nicht erforderlich.

Der mitgelieferte Magier zeigt ein offenes Buch und einen einhändigen Zauberstab. Die zusätzlich im Modell enthaltenen Varianten für das geschlossene Buch und den großen Stab werden ausgeblendet; die Originaldatei bleibt unverändert.

## Mehrere Bots und Stream-Crew

Über die Bot-Auswahl in der linken Seitenleiste oder **Neuen Bot anlegen** einen weiteren Charakter erstellen. Der Dialog enthält Name, mitgelieferte Figur und die Option **Einstellungen übernehmen**. Im Regiepult verwaltet **Deine Stream-Crew** die Figurenfenster und den eigenen OBS-Link jedes Bots.

Mehrere Bots können **gleichzeitig sichtbar im Stream** sein. Jeder hat seine eigene Persönlichkeit, Stimme, Modellwahl, Ansprache, Emotes sowie MCP-/Skill-Freigaben. Die Auswahl in der Seitenleiste bestimmt, welches Profil im Regiepult bearbeitet wird; ein Wechsel schließt die anderen Figurenfenster nicht. Jedes Fenster behält seine Position und erhält nur die Profil-, Sprach- und Emote-Ereignisse seines Bots. OBS-Links enthalten neben der sitzungsgebundenen Kennung eine eigene Bot-ID.

| Beim Anlegen | Ergebnis |
| --- | --- |
| **Frisch beginnen** | Eigenes Profil mit Vorgaben, eigenen `AGENTS.md`-/`PERSONA.md`-Dateien, eigenem Wissen, leerem Verlauf und neuen Schlüsselbund-Referenzen. Installierte Skills und MCP-Server erhalten keine automatische Freigabe. |
| **Einstellungen übernehmen** | Modell-, Stimmen- und weitere Einstellungen, Anweisungen und Wissen werden übernommen; der Verlauf bleibt leer. Der Name in `PERSONA.md` wird angepasst. Skill-/MCP-Freigaben werden als unabhängige Bindungen an dieselbe Bibliothek übernommen. Vorhandene Schlüsselbund-Referenzen werden weiterverwendet; geheime Werte werden nicht kopiert. |

Die App speichert Anweisungen, Wissen und Verlauf pro Bot. Änderungen an `AGENTS.md` oder `PERSONA.md` beeinflussen keine anderen Bots. Ein global bearbeiteter Skill oder MCP-Server ändert dagegen den gemeinsamen Bibliothekseintrag für alle zugeordneten Bots; deren Freigaben bleiben getrennt. Bei übernommenen Einstellungen können beide Bots dieselbe im Schlüsselbund gespeicherte Anbieteranmeldung und dessen Kontingent nutzen.

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

### Bilder aus MCP-Werkzeugen

Unter **Modelle → Bilder an das Modell senden** (`model.visionEnabled`) entscheidet jeder Bot, ob echte Bildeingaben an sein Modell weitergegeben werden dürfen. Das umfasst Bilder aus freigegebenen MCP-Werkzeugen und den eingebauten Bildschirmkontext. Bilddaten werden als Bildanlage übertragen, nicht als Base64-Textbeschreibung.

Mit **Bildschirm als Kontext → Bildschirm bei jeder Anfrage mitsenden** unter **Modelle** oder **Autopilot** nimmt die Desktop-App vor jedem Dialogmodellaufruf einen aktuellen Screenshot auf. Beide Ansichten ändern dieselbe Einstellung des Bots. Dafür sind weder MCP noch Skills oder Werkzeugfreigaben erforderlich. Wähle den Hauptbildschirm oder einen bestimmten Monitor. Bei Cloud-/Remote-Modellen verlassen die Bilddaten deinen Rechner. Screenshots werden nicht im Chatverlauf oder in Hindsight gespeichert; ein bildfähiges Modell und Betriebssystemrechte werden benötigt. Native Aufnahme funktioniert auf macOS, Windows und Linux mit X11; Wayland wird derzeit ausdrücklich abgelehnt. [Einrichtung, Freigabe und Grenzen](SCREEN-CONTEXT.md).

Unterstützt werden OpenAI-kompatible Bildnachrichten einschließlich geeigneter Ollama-/LM-Studio-Endpunkte, Anthropic Messages, Codex App Server und Claude Code mit Stream-JSON-Eingabe. Das **konkret gewählte Modell und der Endpoint müssen Bilder unterstützen**. Eine kompatible Text-API allein genügt nicht. Bekannte reine Textmodelle, deaktivierte Bildfreigabe oder zurückgewiesene Bildanfragen führen zu einem ausdrücklichen Fehler; Bilder werden nicht still durch eine erfundene Analyse ersetzt. Unvollständige Modellmetadaten können die tatsächliche Bildfähigkeit nicht garantieren. Codex weist Bildanfragen auch dann vor der Inferenz ab, wenn die Bildfähigkeit des tatsächlich ausgewählten Modells im CLI-Katalog unbekannt ist.

Pro Gesprächszug gelten über alle Werkzeugrunden hinweg höchstens **vier Bilder**, **5 MiB pro Bild** und **12 MiB Bilddaten insgesamt**. Unterstützte Formate sind PNG, JPEG, WebP und GIF; zusätzlich gelten die Grenzen des Modells. Bei Claude Code darf außerdem die gesamte übergebene JSON-Eingabe inklusive Base64 und Gesprächskontext **10 MiB** nicht überschreiten. Größere Eingaben werden abgewiesen. Es werden ausschließlich eingebettete Bilddaten aus den Werkzeugergebnissen verarbeitet, keine beliebigen Bild-URLs oder lokalen Dateipfade nachgeladen. Bei Cloud-Modellen verlassen diese Bilddaten den Rechner. Bilder und Werkzeugtexte bleiben nicht vertrauenswürdige Inhalte und können keine Berechtigungen verändern.

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

Eine aktive Audioausgabe steuert die Mundbewegung. Bei API-TTS wird die tatsächliche Audioamplitude vor dem Lautstärkeregler gemessen. Vorhandene Vokalmorphs beziehungsweise Kieferknochen öffnen und schließen den Mund, auch bei leiser Sprache; Figuren ohne Mundrig verwenden Sprechgestik. Die wechselnden Mundformen sind eine visuelle Näherung und keine Phonemerkennung. Systemstimmen stellen keine Roh-Audiodaten bereit; ihre Animation verwendet Sprachstart, Wortgrenzen und Sprachende. Der komplette Untertitel wird angezeigt und 15 Sekunden nach dem tatsächlichen Sprachende ausgeblendet. Stop, Audiofehler und eine neue Antwort verwerfen die vorherige Untertitelanzeige; der gespeicherte Gesprächsverlauf bleibt erhalten. Wortgenaue Hervorhebung ist nicht implementiert.

Die Antwort hat zwei Fassungen: `text` enthält den sichtbaren Inhalt, `speechText` die Fassung für die Stimme. Audio-Tags wie `[crying]` oder `[shouts]` bleiben für ElevenLabs im Sprechtext erhalten, erscheinen aber weder im Untertitel noch in neuen Chatantworten oder Erinnerungen. Systemstimmen und kompatible TTS-Anbieter erhalten den bereinigten Text. Tag-Unterstützung und Wirkung hängen vom gewählten ElevenLabs-Modell ab, beispielsweise `eleven_v3`; die App wechselt dein Stimmenmodell nicht automatisch. Die Trennung erfolgt lokal aus einer Modellantwort und benötigt keinen zweiten Dialogaufruf. Eckige Klammern sind außerhalb von Code, Links und nummerierten Quellen für Audio-Tags reserviert.

### Optionale Emote-Sounds

Unter **Charakter & Stimme** kann jedes Emote ohne Sound bleiben, einen integrierten Effekt (**wave**, **cheer**, **whoosh**) verwenden oder einen eigenen Clip mit eigener Lautstärke erhalten. Die integrierten Effekte werden lokal mit WebAudio erzeugt. Eigene **WAV-, MP3-, OGG-, WebM- und M4A-Dateien** werden in den Soundordner dieses Bots importiert; pro Datei sind höchstens **5 MiB** erlaubt. Das jeweilige WebView muss den enthaltenen Audiocodec dekodieren können. Die Ausgabe ist auf **15 Sekunden pro Clip** begrenzt.

Emote-Sounds funktionieren im Studio, im nativen Figurenfenster und in der OBS-Browserquelle. Bei einer geplanten Antwort werden sie vor dem Sprachstart geladen und dekodiert; beim Beginn der Geste startet der vorbereitete Clip parallel. Manuelle Emotes bereiten ihren Sound ebenfalls vor. Pro Präsentation werden höchstens acht unterschiedliche Sounds mit einem begrenzten Decodiercache vorbereitet. Fehlende, gesperrte oder nicht dekodierbare Sounds werden mit Hinweis übersprungen und brechen die Sprachausgabe nicht ab. Falls eine Ausgabe die Audiofreigabe verlangt, zuerst dort klicken. Stop verwirft auch laufende Sounds und verspätete Ladevorgänge.

Unter Linux hängen freie Fensterpositionierung, Transparenz und „immer im Vordergrund“ vom Desktop, X11/Wayland und Compositor ab. Der Browser-Renderer ist eine zusätzliche Stream-Ausgabe unabhängig vom Desktopfenster.

## MCP und Skills

Im Bereich **Skills & MCP** gibt es eine **Bibliothek für alle Bots** und die **Freigaben für den gewählten Bot**. Einen MCP-Server einmal hinzufügen oder einen Skillordner einmal importieren. Das installiert den Eintrag in der App-Bibliothek und aktiviert keinen weiteren Bot. Benötigte Serverprogramme und Laufzeiten müssen separat vorhanden sein; die Bibliothek ist kein Paketmanager.

Für einen MCP-Server **Werkzeuge laden** wählen. Das verbindet den Server und liest seinen Katalog, ohne ein Werkzeug auszuführen oder einen Bot freizugeben. Danach **Für [Bot] verwenden** einschalten und einzelne Werkzeuge in der Spalte **Bot** erlauben. **Chat** erlaubt zusätzlich Zuschauer-Anfragen und setzt die Botfreigabe voraus. Leere Werkzeugfreigaben bedeuten keinen Zugriff. Das Dialogmodell muss zur Integration passendes Tool Calling unterstützen. Eine gemeinsame Serververbindung kann von mehreren Bots genutzt werden; eine Trennung betrifft sie alle. Daten- und Sitzungstrennung im MCP-Server hängen zusätzlich von dessen eigener Implementierung ab.

Für einen Skill **Für [Bot] verwenden** einschalten. Zunächst erhält das Modell nur ID, Name und Beschreibung der freigegebenen Skills. Den vollständigen `SKILL.md`-Text sowie Referenzen lädt es erst bei Bedarf über die Hostwerkzeuge. Das ist schrittweises Laden, kein Training des Modells. `AGENTS.md` und `PERSONA.md` bleiben unter **Persönlichkeit & Regeln** pro Bot editierbar.

**Global bearbeiten** ändert einen Bibliothekseintrag für alle Bots, die ihn verwenden. **Für alle Bots entfernen** löscht ihn aus der Bibliothek und entfernt seine Bindungen. Soll nur ein Bot keinen Zugriff mehr haben, stattdessen dessen Freigabe ausschalten. Neue Bots erhalten ohne Übernahme bestehender Einstellungen keine Erweiterungsfreigaben. Beim ersten Start dieser Version werden bisherige Installationen in die Bibliothek übernommen: identische Inhalte werden zusammengeführt, verschiedene Inhalte bei gleicher Skill-ID behalten getrennte IDs. Die bisherigen Skillordner bleiben bei dieser Migration erhalten.

Lokale MCP-Server benötigen ihre eigenen Laufzeiten. Der Startbefehl wird direkt mit Argumenten aufgerufen, nicht über eine Shell. Für aufwendigere Startumgebungen können absolute Programmpfade verwendet werden. In dateibasierter MCP-Konfiguration können Umgebungswerte auf Schlüsselbund-Einträge verweisen: `${secret:meine-id}`. Profilexporte enthalten keine literalen Umgebungs- oder Header-Geheimnisse.

Unter Windows bevorzugt die App native `.exe`-Programme. Bekannte npm-Startdateien `codex.cmd`, `claude.cmd`, `npm.cmd` und `npx.cmd` werden ohne Batch-Shell über `node.exe` und die zugehörige installierte JavaScript-Datei gestartet. Node.js und die betreffende CLI beziehungsweise der MCP-Server müssen separat installiert sein. Bei anderen Batchwrappern einen echten `.exe`-Pfad verwenden. Für einen manuell eingerichteten npm-MCP-Start kann beispielsweise `C:\Program Files\nodejs\node.exe` als Programm und `C:\Program Files\nodejs\node_modules\npm\bin\npx-cli.js` als erstes Argument eingetragen werden; die Serverargumente folgen als weitere Listeneinträge. Es wird keine zusammengesetzte Shell-Befehlszeile ausgeführt.

`scripts/`, `references/` und `assets/` werden mit dem Skillordner importiert. **Skill-Skripte für [Bot] erlauben** ist eine zusätzliche Freigabe pro Bot, standardmäßig aus. Skripte benötigen eine separat verfügbare Node-/Python-Laufzeit und dürfen nur bei ausdrücklichen Betreiber-Aufträgen laufen; Zuschauer erhalten diese Fähigkeit nicht. Die App bietet dafür keine Betriebssystem-Sandbox. Ein Profilpaket enthält nur die diesem Bot zugewiesenen Erweiterungen und deren Ressourcen; Zugangsdaten müssen am Ziel erneut hinterlegt werden. Ein Beispielprofil liegt unter `character-template/`.

Importierte Profile erhalten neue, ungebundene Zugangsdaten-Verweise. Sie können keine vorhandenen Schlüssel oder automatisch erkannten Standard-Schlüssel mitbenutzen. Auch eigene CLI-Programmpfade werden zurückgesetzt; richte sie nach dem Import erneut ein. MCP-Zugriff, Skill-Skripte, Hindsight und automatische Plattformantworten bleiben bis zu deiner Freigabe aus. Das ausdrückliche Kopieren eines eigenen Bots innerhalb der App behält dagegen die gewählten Verweise.

### Den Cohost über einen Agenten steuern

Unter **Skills & MCP → Den Cohost per Agent steuern → MCP + Skill installieren** werden der eingebaute Steuerungsserver und der passende `cohost-control`-Skill in die gemeinsame Bibliothek aufgenommen. Die Aktion startet keinen Dialog, führt keine Werkzeuge oder Skripte aus und erteilt keine Botrechte. Wähle danach beispielsweise Nova, aktiviere für sie den Skill und den Server und gib die gewünschten MCP-Werkzeuge einzeln frei. Zuschauerrechte bleiben eine zusätzliche bewusste Freigabe.

| Werkzeug | Wirkung |
| --- | --- |
| `cohost_status` | Öffentlicher Appstatus und Auslastung der Dialog-Laufzeit. |
| `cohost_list_bots` | Bis zu 100 Bots mit ID, Namen und Ausgabestatus; keine Verläufe, Dateipfade oder Zugangsdaten. |
| `cohost_list_emotes` | Für den Zielbot über diesen Server erlaubte Emotes. |
| `cohost_perform_emote` | Vorhandenes Emote mit dessen konfiguriertem optionalem Sound auslösen: intern zum Antwortstart einplanen, extern sofort übergeben. |
| `cohost_show_character` / `cohost_hide_character` | Natives Figurenfenster öffnen und Overlay wählen beziehungsweise Figurenausgabe beenden und Studio wiederherstellen. |
| `cohost_pause_auto_replies` | Das offene Regiepult auffordern, automatische Antworten zu pausieren. |
| `cohost_stop_output` | Aktive Generierung und ausstehende/spielende Ausgaben aller Bots stoppen. |

Der allgemeine Steuerungsserver führt nur Emotes mit **Zuschauerfreigabe** (`allowChat=true`) aus, auch bei externen Betreiber-Clients. So kann eine MCP-Freigabe die Emote-Freigabe nicht umgehen. Eine Emote-Dispatch-Bestätigung beweist noch keine abgeschlossene Wiedergabe. Anzeigen und Ausblenden liefern während einer laufenden Antwort oder vorbereiteten/laufenden Sprache des Zielbots einen ausdrücklichen Beschäftigt-Fehler. Das betrifft auch entsprechende Werkzeugaufrufe aus einem eigenen Botturn. Ein Pausenergebnis bestätigt nur die Übergabe der Anforderung, keine Rückmeldung des Regiepults und keine dauerhaft gespeicherte Änderung.

Ein interner Cohost-Bot darf über `cohost_perform_emote` ausschließlich seinen eigenen Charakter steuern. Nach der Werkzeugfreigabenprüfung plant die Agenten-Laufzeit das Emote einschließlich Sound für den tatsächlichen Sprach-/Textausgabestart ein. Während der Denkphase wird keine Geste vorab gezeigt; Stop verwirft geplante Emotes. Ein externer MCP-Client mit Steuerungstoken kann dagegen das gewählte erlaubte Emote sofort an die Ausgabe übergeben, ohne einen Modellturn auszulösen.

Für externe Agenten **Claude / MCP-JSON** oder **Codex · TOML** wählen und **Verbindung kopieren** anklicken. Die Konfiguration im dafür vorgesehenen Bereich des externen Clients einfügen; dessen Einrichtung bleibt ein separater Schritt. Der Server ist nur auf dem lokalen Rechner erreichbar, benötigt die laufende App und prüft Host, Origin und einen eigenen Bearer-Token. **Port und Steuerungstoken wechseln bei jedem App-Neustart**: danach die Konfiguration erneut kopieren. Der kopierte Token erlaubt die angebotenen Steuerungswerkzeuge als Betreiberzugang; die Botfreigaben der internen Bibliothek beschränken externe Clients nicht. Nicht im Stream, in Profilpaketen oder öffentlich teilen. Ein OBS-Link enthält einen anderen Zugang und berechtigt nicht zur Steuerung.

Der Server bietet keine neuen Modellanfragen, allgemeinen Nachrichtenversand, freien Dateizugriff, Einstellungen, Promptänderungen oder Rechtevergabe an. Der Skill enthält die Bedienhinweise; er benötigt keine Skriptausführung. Die Einbindung selbst erzeugt keine automatischen Aktionen.

## Wissen und Hindsight

Im Bereich **Wissen & Gedächtnis** PDF, TXT, Markdown, JSON oder CSV importieren. PDF-Dateien sind auf 20 MiB und 500 Seiten begrenzt, extrahierter Text und UTF-8-Dateien auf 4 MiB; pro Bot maximal 128 Dokumente und 16 MiB indizierter Text. Die App erhält die Originaldatei und indexiert den Text beim Import. Scans ohne Text benötigen vorher OCR. Gleichnamige Dateien werden nicht überschrieben.

Vor jeder Antwort sucht die App lokal nach passenden Stichwörtern und fügt begrenzte Auszüge automatisch ein, auch bei ausgeschalteter Werkzeugnutzung. Quellen tragen Dateiname, Textzeilen und bei PDFs eine Seitennummer. PDF-Zeilen beziehen sich auf den extrahierten Text. Das Modell wird angewiesen, beim Bezug darauf die Quelle zu nennen; die tatsächliche Befolgung hängt vom Modell ab. Die zusätzliche Wissenssuche als Werkzeug ermöglicht weitere Nachfragen. Bei einem Cloud-Dialogmodell gehen relevante Auszüge an dessen Anbieter. Wissen bleibt getrennt vom Langzeitgedächtnis und wird mit dem Profil exportiert.

**Hindsight** ist das einzige Langzeitgedächtnis. Es ist standardmäßig aus. Eigene Server-URL und optional API-Schlüssel eintragen, automatische Erinnerungssuche und Speicherung getrennt wählen und speichern. Ohne Bank-ID wird die Bot-ID verwendet. Neue Bots erhalten eigene Banken; dieselbe Bank-ID teilt Erinnerungen absichtlich. Importierte Profile müssen Hindsight erneut aktivieren.

Der Recall ergänzt relevante Erinnerungen vor der Antwort; bei Fehlern läuft das Gespräch mit einer sichtbaren Aktivitätsmeldung weiter. Neue Nutzer-Nachrichten und Antworten werden erst beim tatsächlichen Ausgabestart einmalig zum Speichern eingereicht. Eine angenommene asynchrone Speicheroperation bedeutet noch nicht, dass Hindsight sie fertig verarbeitet hat. Es werden keine alten Verläufe, Wissensdateien oder Profile gesammelt hochgeladen. Erinnerungen werden auf dem Hindsight-Server verwaltet und gelöscht. Auch ein lokaler Hindsight-Server kann konfigurierte Cloud-Modelle und Embeddings verwenden. Der lokale SQLite-Verlauf bleibt als begrenzter Kurzzeitkontext erhalten; **Verlauf leeren** löscht keine Hindsight-Erinnerungen.

## Aktivierungsphrase am Mikrofon

Die Mikrofon-Statuszeile enthält aufklappbare Einstellungen für eigene Phrasen, etwa **hey nova**. Die Erkennung startet nur durch bewusstes Einschalten des Mikrofons. Nach der Phrase wird der folgende Text als Nachricht verarbeitet; bei einer Phrase ohne weiteren Text öffnet sich ein kurzes Nachfragefenster. Während laufender Antwort, Sprachausgabe oder manueller Aufnahme pausiert die Erfassung. Stop und Botwechsel beenden sie.

Nach einer Antwort- oder Emote-Soundpause hört eine bewusst gestartete Sitzung automatisch weiter. Ein einzelner Erkennungs-, Verbindungs- oder Aufnahmefehler beendet sie nicht: Wiederholungen warten 1, 2, 4, 8 und 16 Sekunden. Nach fünf erfolglosen Wiederaufnahmen erscheint ein Hinweis zum erneuten Start. Entzogener Mikrofonzugriff und ein getrenntes Gerät benötigen einen bewussten Neustart.

Die Funktion erkennt Phrasen in den Transkripten kurzer VAD-Aufnahmen. Sie ist kein separates Offline-Wake-Word-Modell. Bei Cloud-STT werden Audioabschnitte schon vor Erkennung der Phrase an den ausgewählten Anbieter geschickt. Für lokale Verarbeitung einen lokalen Whisper-Server verwenden. Gespeicherte Aktivierung startet nach einem App-Neustart kein Mikrofon automatisch.

## Antworttempo und CLI-Modelle

Unter **Modelle** stehen für Codex **Modelle & Denkstufen laden** und für Claude Code bekannte Modellvorgaben plus eigene Modell-ID bereit. Das Codex-Katalogergebnis kann aus der CLI-Konfiguration beziehungsweise einem Cache stammen und ist keine Zusage für Kontozugriff. Denkstufen werden passend zum ausgewählten Modell angeboten; **CLI-Vorgabe** überschreibt keine Vorgabe der CLI. Die Schnellvorgabe verwendet bei Claude Haiku, bei Codex eine niedrige angebotene Denkstufe sowie kurze Antworten, eine Werkzeugrunde und zwölf Kontextnachrichten. Änderungen speichern. Modellgeschwindigkeit, Werkzeug- und TTS-Laufzeit bleiben anbieterabhängig.

## Entwicklung

Siehe [CONTRIBUTING.md](../CONTRIBUTING.md) für Installation, lokale Tests und Builds, [ARCHITECTURE.md](ARCHITECTURE.md) für die Komponenten und [RELEASING.md](RELEASING.md) für automatische Pakete.

## Daten und Grenzen dieser Version

Die App legt Charakterdateien, Wissensdateien, Emote-Sounds und die SQLite-Datenbank in ihrem Betriebssystem-App-Datenordner ab. Skills liegen gemeinsam unter `library/skills/`, MCP-Definitionen im gemeinsamen Datenbankkatalog; Botfreigaben, Anweisungen und Verlauf bleiben getrennt. Der Datenordner ist in der Oberfläche zugänglich. API-Schlüssel und Tokens liegen separat im Betriebssystem-Schlüsselbund. Profile werden als JSON-Bundle einschließlich der zugewiesenen Erweiterungen, verwalteten Charakterressourcen, Wissensdateien und Sounds exportiert.

Cloud- und lokale Stimmen sind getrennte Ressourcen. Eine ElevenLabs-Stimme wird nicht als lokales Stimmenmodell heruntergeladen. Benutzer stellen eigene Anbieterzugänge, lokale Modelllaufzeiten, gegebenenfalls CLI-Installationen mit eigener Anmeldung, STT-Server und Whisper-Modelle, Stimmen und eigene 3D-Dateien bereit. CLI-Anmeldung und Zugriff auf Cloud-STT sind unabhängig; ein Codex- oder Claude-Code-Login ersetzt keinen OpenAI-/ElevenLabs-STT-API-Schlüssel.

Eigenes Modelltraining, automatische Token-Erstellung bei Streaming-Plattformen und die Erzeugung eines individuellen 3D-Modells aus Fotos sind nicht Bestandteil dieser Version. Angepasste Modelle können über die jeweiligen Modelllaufzeiten eingebunden werden. Automatisierte Tests verwenden lokale Fixtures und Mocks; sie ersetzen keinen Live-Test mit dem eigenen Stream-Setup. Streaming-Konten, Modellanbieter, TTS/STT-Dienste und fremde MCP-Server wurden nicht live verifiziert. Der native Build belegt keine gemessene Leistung oder erfolgreiche Live-Anbieteranbindung.

Die Linux-GTK-Abhängigkeit GLib 0.18.5 enthält einen dokumentierten Upstream-Backport für **GHSA-wrw7-89jp-8q8g**. Linux-CI und Release-Pipeline prüfen den betroffenen Iterator optimiert und kontrollieren die Quellhashes. Die Versionsnummer bleibt unverändert; reine Versionsscanner können den Befund daher weiter anzeigen. Details stehen im [Backport-Protokoll](../vendor/glib/COHOST-BACKPORT.md) und in [SECURITY.md](../SECURITY.md).

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
