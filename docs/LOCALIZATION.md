# Sprachen der Oberfläche

OrbitPal bietet Deutsch, English, Français und Español. Die Auswahl steht im Kopfbereich des Studios und gilt sofort für Navigation, Einstellungen, Hilfetexte, app-eigene Statusmeldungen und die nativen Fenstermenüs. Geöffnete Figurenfenster folgen der Auswahl. Auf macOS werden die Menüeinträge im Menüleisten-Symbol ebenfalls aktualisiert.

Beim ersten Start wird eine passende Sprache aus der Betriebssystem-/Browserpräferenz gewählt. Regionale Varianten wie `de-AT` oder `fr-CA` werden der jeweiligen Sprache zugeordnet. Ohne passende Sprache startet die Oberfläche auf Englisch. Eine manuelle Auswahl bleibt auf diesem Rechner gespeichert; sie ist unabhängig vom gewählten Bot.

Die Sprache des Gesprächs wird weiterhin durch die Persönlichkeit, die Bot-Anweisungen und die gewählten Sprachdienste bestimmt. Ein Wechsel der Oberfläche übersetzt weder Chatnachrichten noch Wissen, Erinnerungen, eigene Emotes, `AGENTS.md`, `PERSONA.md` oder Skill-Dateien. Auch die gewählte Stimme und die Sprache der Spracherkennung bleiben erhalten. Eigene Modell- und Clipnamen sowie externe Provider-Fehlermeldungen werden nicht umgeschrieben.

## Übersetzungen pflegen

Die Kataloge liegen in `src/lib/locales/`. Deutsche Quelltexte dienen als Schlüssel; jeder Schlüssel hat englische, französische und spanische Einträge. Svelte-Komponenten verwenden den reaktiven Store `$t`, imperative App-Meldungen `translate`. Zahlen und Datumsangaben werden über `formatNumber` beziehungsweise `formatDate` formatiert.

Benannte Platzhalter wie `{name}` müssen in allen Übersetzungen erhalten bleiben. Inhalte aus Profilen oder externen Diensten werden als Werte eingefügt und nicht erneut als Übersetzung ausgewertet. HTML darf nicht als Übersetzung interpretiert werden. Technische IDs, API-Felder, URLs und gespeicherte Profile bleiben sprachneutral.

`pnpm check` und `pnpm test:run` prüfen unter anderem die Kataloggleichheit, Platzhalter und die literalen Übersetzungsaufrufe in den Ansichten. Neue oder längere Texte sollten zusätzlich in allen vier Sprachen im Studio geprüft werden. Die native Menüübersetzung liegt separat in `src-tauri/src/ui_locale.rs`; Änderungen daran benötigen auch die nativen Prüfungen.

Die bisherige lokale Einstellungskennung `streamer-ai-cohost.ui-language` bleibt als technische Kennung erhalten, damit die Umbenennung vorhandene Einstellungen nicht verwirft.
