# Bildschirm als Kontext

Die Desktop-App kann vor jedem Aufruf des Dialogmodells einen frischen Screenshot aufnehmen und als echtes Bild beifügen. Das gilt für normale Nachrichten, Mikrofoneingaben, Antworten auf Twitch-/YouTube-Nachrichten und spontane Autopilot-Beiträge. Auch nach einer Werkzeugrunde bekommt der nächste Modellaufruf ein aktuelles Bild. MCP-Server, Skills und Werkzeugfreigaben sind dafür nicht erforderlich.

## Einrichten

1. Den Bot auswählen und unter **Modelle** oder **Autopilot** die Einstellung **Bildschirm bei jeder Anfrage mitsenden** aktivieren. Beide Ansichten verwenden dieselbe Einstellung dieses Bots.
2. Ein bildfähiges Dialogmodell verwenden und **Bilder an das Modell senden** erlauben. Ein reines Textmodell erhält dadurch keine Sehfähigkeit. Unterstützte API-, lokale, Codex- und Claude-Code-Adapter verwenden ihre bestehende Bildübertragung.
3. Den **Hauptbildschirm** verwenden oder mit **Bildschirme laden** einen bestimmten Monitor auswählen. Das Laden der Liste nimmt noch kein Bild auf. Einen nicht mehr verfügbaren ausgewählten Monitor ersetzt die App nicht unbemerkt durch einen anderen.
4. Änderungen speichern. Je nach Betriebssystem muss die App zusätzlich für Bildschirmaufnahmen freigegeben werden. Die Browser-Vorschau kann keine native Aufnahme erstellen.

## Umfang und Daten

Es wird jeweils ein einzelnes Bild unmittelbar vor einem Dialogmodellaufruf aufgenommen, kein Bildschirmvideo an das Modell gestreamt. Modelllisten, Hindsight-Abrufe, Spracherkennung und Sprachausgabe lösen keine Bildschirmaufnahme aus. Jede Werkzeug-Folgerunde verwendet einen neuen Screenshot; frühere Bildschirmbilder werden nicht in der nächsten Anfrage mitgeführt.

Bei Cloud- oder Remote-Anbietern verlassen die Bilddaten den Rechner. Das gilt auch für selbst konfigurierte entfernte kompatible Endpunkte. Der gesamte ausgewählte Monitor kann private Inhalte enthalten. Die Bildschirmfreigabe erlaubt weder Klicken noch Computersteuerung und aktiviert keine zusätzlichen MCP-Werkzeuge oder Skill-Skripte.

Die App hält Screenshots im Arbeitsspeicher, verkleinert sie auf höchstens 1.280 Pixel Kantenlänge und überträgt sie als begrenztes JPEG. Bilddateien, Bildverlauf und Bildspeicherung in Hindsight werden nicht angelegt. Normale Antworten können Bildinhalte beschreiben und werden weiterhin entsprechend den Gesprächs- und Gedächtniseinstellungen gespeichert. Für die Verarbeitung und Speicherung beim gewählten Anbieter gelten dessen Einstellungen.

Ein Screenshot belegt einen der höchstens vier Bildplätze eines Modellaufrufs. Vorhandene MCP-Bildbelege teilen sich dieses Limit sowie die Grenze von 5 MiB je Bild und 12 MiB insgesamt. Claude Code begrenzt zusätzlich den gesamten JSON-Eingabetext einschließlich Base64-Bildern. Die App meldet überschrittene Grenzen; sie ersetzt Bilder nicht durch erfundene Beschreibungen.

## Freigabe und Fehler

Die Freigabe gehört zum Bot. Importierte oder neu kopierte Bots erhalten sie nicht automatisch; auch die ältere Autopilot-MCP-Bildschirmfreigabe aktiviert die native Aufnahme nicht. Änderungen an Bildschirmfreigabe, Monitor oder Modellziel widerrufen laufende Anfragen dieses Bots. Das verhindert, dass eine verspätete Aufnahme nach Aus- und Einschalten wieder an ein Modell gesendet wird. Andere Bots werden davon nicht abgebrochen.

Fehlende Betriebssystemrechte, ein verschwundener Monitor, Aufnahmefehler oder fehlende Bildunterstützung brechen die betroffene Anfrage mit einer konkreten Fehlermeldung ab. Es wird kein altes Bild und kein stiller Textmodus als Ersatz verwendet. Ein bereits an einen Anbieter gesendetes Bild lässt sich durch späteres Ausschalten nicht zurückholen.

## Betriebssysteme

- **macOS:** native CoreGraphics-Aufnahme; die Bildschirmaufnahme-Freigabe muss vom Betriebssystem erteilt sein.
- **Windows:** native GDI-Aufnahme des ausgewählten Monitors. Geschützte Inhalte können vom Betriebssystem ausgenommen werden.
- **Linux mit X11:** native X11/RandR-Aufnahme ohne zusätzlichen MCP-Server.
- **Linux mit Wayland:** derzeit nicht unterstützt. Die App lehnt die Aufnahme ausdrücklich ab und verwendet keinen unvollständigen XWayland-Bildschirm. Für eine spätere Unterstützung ist ein eigener ScreenCast-/PipeWire-Aufnahmeablauf mit Systemauswahl und Sitzungsfreigabe erforderlich.
