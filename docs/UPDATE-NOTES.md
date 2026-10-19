OrbitPal 0.10.0

Der Autopilot zeigt jetzt, warum er wartet und wann der nächste Sprechversuch ansteht. Er kann auch ohne neue Chatnachricht eine passende Frage beginnen. Automatische Lauf- und Gestensounds verhindern solche Beiträge nicht mehr. Pausieren und Neustarten verwerfen alte ausstehende Anfragen.

Die Desktopfigur darf kurz seitlich verschwinden. Vor Sprache, Text oder einer ausdrücklich ausgelösten Geste kehrt sie vollständig sichtbar zurück und bleibt stehen. Erst danach startet die vorbereitete Ausgabe. Stop verwirft wartende Antworten.

Separate VRMA-1.0-Bewegungen lassen sich pro Bot importieren und mit unterstützten VRM-0/1-Figuren verwenden. Komprimierte Avatare mit Draco, Meshopt und BasisU/KTX2 werden offline geladen. Emotes und Sounds werden gegen die tatsächlich geladene Figur geprüft. Eingebettete Animationssounds unterstützen ein begrenztes Profil des MSFT-Audioentwurfs, keinen allgemeinen glTF-/VRM-Soundstandard.

Fehlende Mikrofon- oder Bildschirmrechte führen bei bewusst gestarteten Funktionen durch Anfrage, Systemeinstellungen und erneute Prüfung. Die Rückkehr aus den Einstellungen startet keine Aufnahme. Unter Wayland gibt es eine ausdrückliche Quellenauswahl über ScreenCast/PipeWire; diese Aufnahme bleibt bis zur realen GNOME-/KDE-Abnahme experimentell.

Update-Download und Prüfung können vor Installationsbeginn abgebrochen werden. Danach ist für einen neuen Versuch eine erneute Prüfung und Bestätigung nötig. Die Installation selbst bleibt nicht abbrechbar. Release-Pakete werden vor Veröffentlichung zusätzlich auf Signatur und Build-Herkunft geprüft.

macOS verwendet erstmals eine dauerhafte selbstsignierte Identität. Beim Wechsel von 0.9.0 oder älter können Mikrofon-/Bildschirmrechte einmal erneut nötig sein; ihr Erhalt über Updates ist noch nicht praktisch bestätigt. Die App ist nicht von Apple notarisiert. Windows-Installer haben weiterhin keine Authenticode-Signatur. Vorhandene Bots, Profile, Zugangsdaten und die separate Updater-Vertrauensbasis bleiben erhalten.

Signierungs-/Installationsabnahme (#17), ausstehende Rust-Abhängigkeitsmigrationen (#19) und echte Wayland-Abnahme (#26) bleiben offen. Automatische Prüfungen ersetzen keine echte Bildschirm-, Mehrmonitor-, OBS-/GPU- oder installierte Update-Prüfung.
