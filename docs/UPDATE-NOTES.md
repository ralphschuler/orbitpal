Streamer AI Cohost 0.7.0

Bildschirm als Kontext ist jetzt eine eingebaute Funktion pro Bot: Vor jedem Dialogmodellaufruf – auch normalen Chats, Autopilot und Werkzeug-Folgerunden – wird ein frischer Screenshot angehängt. MCP-Server und Skills werden nicht benötigt. Wähle unter Modelle oder Autopilot den Hauptbildschirm oder einen bestimmten Monitor und verwende ein bildfähiges Modell. Cloud-/Remote-Anbieter erhalten die Bilddaten; die App speichert keine Screenshots im Chatverlauf oder Gedächtnis. Unterstützt sind macOS, Windows und Linux X11; Wayland wird derzeit mit einer klaren Meldung abgelehnt. [Einrichtung und Grenzen](SCREEN-CONTEXT.md).

Figuren kehren nach dem Laufen und nach eigenen Animationsclips wieder sauber in ihre mitgelieferte Idle-Animation zurück. T-Posen durch deaktivierte Idle-Aktionen und Lücken beim Ausblenden von Gesten sind behoben.

Die App kann neue stabile Versionen über GitHub Releases erkennen. Herunterladen und Installieren beginnt erst nach deiner ausdrücklichen Bestätigung. Vor der Installation werden die Signatur und die darin gebundene Versionsnummer geprüft.

Das Update beendet die laufende App und startet die neue Version. Speichere Änderungen und beende laufende Ausgaben und Mikrofonaufnahmen vor der Installation. Unter Linux wird die Installation innerhalb der App für AppImage unterstützt; Debian-Pakete aktualisierst du manuell.

Updater-Signaturen sind unabhängig von der Signatur des Betriebssystems. Windows-Installer besitzen weiterhin keine Authenticode-Signatur; macOS-Builds sind ad-hoc signiert und nicht von Apple notarisiert.

Beim Import eines Profils werden Zugangsdaten nicht übernommen oder mit bestehenden Schlüsseln verknüpft. Richte sie und gegebenenfalls eigene CLI-Pfade nach dem Import neu ein. Mikrofon-Anfragen und vorbereitete Emote-Sounds werden nach Stop oder Update-Bestätigung verworfen. Profil-Exporte speichern auch geänderte Persönlichkeits- und Agenten-Anweisungen.

Die zehn OSA-Figuren halten ihre Arme jetzt in der passenden Ruhepose und winken nach oben. Beim Magier wird nur eine Buch- und Stabvariante gezeigt.

Autopilot ergänzt bewusst gestartete Sitzungen pro Bot: gelegentliches Herumlaufen, stumme Gesten und eigene Beiträge mit Pausen und Stundenbudget. [Einrichtung und Grenzen](AUTOPILOT.md). Eigene Modellclips lassen sich direkt ansehen und als Emote übernehmen; [unterstützte Avatarformate](AVATAR-SUPPORT.md) werden beim Import geprüft.
