# Updates

Ab Version 0.6.0 prüft OrbitPal auf neue stabile Versionen aus den [offiziellen GitHub Releases](https://github.com/ralphschuler/orbitpal/releases). Die Prüfung lädt zunächst nur die Versionsinformationen. Erst deine Bestätigung erlaubt das Herunterladen und Installieren eines Updates. Du kannst ein Angebot verschieben und später erneut prüfen.

## Vor der Installation

Speichere deine Änderungen und beende laufende Antworten, Audioausgaben und Mikrofonaufnahmen. Die Installation ersetzt die Anwendung und benötigt einen Neustart. Die App bietet keinen automatischen Start des Mikrofons nach dem Neustart. Ein Stream mit diesem Co-Host sollte deshalb während einer geplanten Pause aktualisiert werden.

Netzwerk- oder Signaturfehler werden angezeigt. Bei einem Fehler wird keine ungeprüfte Datei installiert. Nutze bei anhaltenden Problemen die passende Datei auf der offiziellen Release-Seite und melde den Fehler ohne Zugangsdaten, private Chatverläufe oder exportierte Profile.

## Unterstützte Installationen

| System | Installation des Updates |
| --- | --- |
| Windows x64 | Passender NSIS- oder MSI-Installer |
| macOS Apple Silicon / Intel | Signiertes Updater-Archiv ersetzt das installierte App-Bundle |
| Linux x64 AppImage | Ersetzt das beschreibbare AppImage |
| Linux Debian-Paket | Neue `.deb`-Datei von der Release-Seite manuell installieren |

Die Anwendung muss an einem beschreibbaren Ort installiert sein. Starte eine macOS-Anwendung aus dem installierten App-Bundle und nicht aus dem schreibgeschützten DMG. Unter Linux muss das tatsächlich ausgeführte AppImage ersetzt werden können.

Versionen vor 0.6.0 enthalten keinen Updater. Installiere einmal manuell Version 0.6.0 oder neuer; danach steht die Update-Funktion zur Verfügung. Selbst gebaute Entwicklungsfassungen und reine Browser-Vorschauen sind kein Ersatz für die veröffentlichten installierbaren Pakete.

## Herkunft und Freigabe

Die App verwendet den festen GitHub-Release-Kanal dieses Projekts und bietet nur neuere stabile Versionen an. Prereleases werden nicht automatisch angeboten. Jedes Update-Paket benötigt eine gültige Tauri-Signatur zum eingebauten öffentlichen Schlüssel sowie die zur angebotenen Version passende signierte Versionsnummer. MCP-Server, Skills, Chatnachrichten und Charaktere bekommen keine Berechtigung, selbst Updates auszulösen.

Diese Update-Signatur bestätigt das Paket innerhalb der Anwendung. Windows-Installer haben weiterhin keine Authenticode-Signatur; macOS-Builds sind ad-hoc signiert und nicht Apple-notarisiert. Details zur Veröffentlichung und zum geschützten Signierschlüssel stehen im [Release-Handbuch](RELEASING.md).

## Neuer Name ab 0.8.0

OrbitPal hieß bisher Streamer AI Cohost. Profile, Schlüsselbund und Freigaben bleiben erhalten. Ein Update kann den bisherigen App-/AppImage-Dateinamen beibehalten. Lade neue Pakete ausschließlich aus dem offiziellen Repository `ralphschuler/orbitpal`. Details zu alten Download-Weiterleitungen und Windows-Installationsidentität stehen in [MIGRATING-TO-ORBITPAL.md](MIGRATING-TO-ORBITPAL.md).
