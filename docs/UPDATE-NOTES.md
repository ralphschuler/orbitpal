OrbitPal 0.11.1

Der Desktop-Autopilot richtet die Figur jetzt auch beim Laufen nach oben, unten und diagonal nach ihrem tatsächlichen Weg aus. Eine leicht erhöhte Ansicht macht die Schritte in die Tiefe sichtbar; der Körper bleibt aufrecht. Ausflüge aus dem Bild, die Rückkehr vor Sprache oder Emotes, Ruheposen und die live einstellbare Figurgröße bleiben erhalten.

Das Schritttempo folgt der zurückgelegten Strecke und der Figurgröße, auch beim Beschleunigen, Bremsen und am Bildschirmrand. Pausen spulen die Laufanimation nicht mehr weiter. Vorhandene Laufclips werden automatisch vermessen; bei ungewöhnlichen Modellen bleibt die Schrittlänge eine Näherung.

Leere oder ausschließlich aus Emotions-Tags bestehende Autopilot-Antworten werden als Fehler gemeldet. Das Modell darf ausdrücklich schweigen; dabei entstehen wie bisher keine zusätzlichen automatischen Sprechversuche.

Das Update startet erst nach ausdrücklicher Bestätigung. Linux-AppImages unterstützen das App-Update; DEB-Pakete werden manuell aktualisiert. Bestehende Profile, Bot-Freigaben, Zugangsdaten-IDs und die Updater-Vertrauensbasis bleiben kompatibel.

macOS verwendet dieselbe dauerhafte selbstsignierte Identität und ist nicht von Apple notarisiert. Windows-Installer haben keine Authenticode-Signatur. Der Erhalt von Mikrofon-/Bildschirmrechten über installierte Updates ist weiterhin nicht praktisch bestätigt; bei alten ad-hoc signierten Versionen bis 0.9.0 können neue Freigaben nötig sein.

Die Darstellung wurde mit lokalen Charakter-Testdaten geprüft. Echte Desktopfenster, Betriebssystemrechte, reale Audiowiedergabe und Anbieter wurden dabei nicht verwendet. Offen bleiben Signierungs-/Installationsabnahme (#17), Rust-Abhängigkeitsmigrationen (#19) und die vollständige Wayland-Abnahme (#26). Wayland bleibt experimentell.
