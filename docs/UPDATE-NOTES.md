OrbitPal 0.10.1

Die Wayland-Quellenauswahl speichert zuerst die aktuellen Bot- und Modelleinstellungen. Veraltete Ergebnisse werden verworfen und schließen nur ihre eigene Freigabesitzung. Nach der Auswahl die neue Quelle noch einmal speichern.

Bestehende Zugangsdaten behalten ihre IDs und Speicherzuordnungen. Die neuen nativen Adapter zeigen übersetzte Fehlerkategorien, ohne geheime Werte oder rohe Backend-Meldungen offenzulegen.

Für GNOME 46 und KDE Plasma 5.27 gibt es isolierte Prüfungen mit echten Freigabedialogen und frischen Aufnahmen eines synthetischen Desktops. Wayland bleibt bis zur vollständigen Desktop-Abnahme experimentell.

Das Update erfolgt erst nach ausdrücklicher Bestätigung. Linux-AppImages unterstützen das App-Update; DEB-Pakete werden manuell aktualisiert. Profile, Bot-Freigaben und die Updater-Vertrauensbasis bleiben erhalten.

macOS verwendet dieselbe dauerhafte selbstsignierte Identität wie 0.10.0 und ist nicht von Apple notarisiert. Windows-Installer haben keine Authenticode-Signatur. Der Erhalt von Mikrofon-/Bildschirmrechten über installierte Updates ist noch nicht praktisch bestätigt.

Offen bleiben Signierungs-/Installationsabnahme (#17), weitere Rust-Abhängigkeitsmigrationen (#19) und die vollständige Wayland-Abnahme (#26).
