OrbitPal 0.10.1

Die Wayland-Quellenauswahl speichert zuerst die aktuellen Bot- und Modelleinstellungen. Veraltete Ergebnisse werden verworfen und schließen nur ihre eigene Freigabesitzung. Nach der Auswahl die neue Quelle noch einmal speichern.

Bestehende Zugangsdaten behalten ihre IDs und Speicherzuordnungen. Die neuen nativen Adapter zeigen übersetzte Fehlerkategorien, ohne geheime Werte oder rohe Backend-Meldungen offenzulegen.

Wayland bleibt experimentell. Ein separater GNOME-Test konnte frische synthetische Bilder aufnehmen; die KDE-Abnahme ist durch eine Einschränkung des Test-Compositors blockiert. Der unfertige Desktop-Test bleibt separat in Entwurfs-PR #56.

Das Update erfolgt erst nach ausdrücklicher Bestätigung. Linux-AppImages unterstützen das App-Update; DEB-Pakete werden manuell aktualisiert. Profile, Bot-Freigaben und die Updater-Vertrauensbasis bleiben erhalten.

macOS verwendet dieselbe dauerhafte selbstsignierte Identität wie 0.10.0 und ist nicht von Apple notarisiert. Windows-Installer haben keine Authenticode-Signatur. Der Erhalt von Mikrofon-/Bildschirmrechten über installierte Updates ist noch nicht praktisch bestätigt.

Offen bleiben Signierungs-/Installationsabnahme (#17), weitere Rust-Abhängigkeitsmigrationen (#19) und die vollständige Wayland-Abnahme (#26).
