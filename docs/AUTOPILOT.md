# Autopilot

Der Autopilot gibt einem Bot gelegentlich selbst einen Anlass für Bewegung oder einen eigenen Beitrag. Er verwendet weiterhin dessen Dialogmodell, Stimme, `AGENTS.md`, `PERSONA.md`, freigegebene Skill-Texte, Wissen und optional Hindsight.

## Start und Stop

1. Den Bot auswählen und **Autopilot** öffnen.
2. **Herumlaufen**, **Kleine Gesten** und **Von selbst sprechen** unabhängig einstellen. Für reine Bewegung ist kein Dialogmodell nötig. **Bewegung ansehen** zeigt eine stumme, auf 30 Sekunden begrenzte Vorschau ohne Modell- oder Bildschirmaufruf.
3. Für eigene Beiträge ein Dialogmodell und bei Bedarf eine Stimme konfigurieren. Die zusätzlichen Hinweise ergänzen die vorhandenen Charakterregeln.
4. **Autopilot starten** speichert die Einstellungen und aktiviert diesen Bot für die laufende App-Sitzung. Andere Bots können separat gestartet werden, wenn ihre Figur im Studio, Desktopfenster oder in OBS ausgegeben wird.
5. **Autopilot pausieren** beendet diesen Bot. **Stop** beziehungsweise Escape außerhalb eines Dialogs beendet alle Autopiloten und laufenden Ausgaben. Nach einem App-Neustart ist ein neuer bewusster Start nötig.

Die Konfiguration gehört zum Bot; der aktive Laufstatus wird nicht im Profil gespeichert oder importiert. Änderungen werden über **Änderungen speichern** übernommen. Ein Update kann erst installiert werden, wenn die Autopiloten und übrigen laufenden Aktivitäten beendet sind.

## Bewegung und Darstellung

Im **Studio und in OBS** bewegt sich die Figur mit Ruhepausen innerhalb ihrer vorhandenen Bühne. Deren Kamera und Laufweg bleiben erhalten. Für sichtbare Schritte ist **Ganze Figur** unter **Charakter & Stimme** sinnvoll.

Im **nativen transparenten Figurenfenster** erweitert aktives **Herumlaufen** die Ausgabe dagegen auf die Fläche eines Monitors. Es entsteht kein eigener macOS-Vollbildbereich. Die Figur betritt die Fläche von links oder rechts und besucht mit Pausen Punkte in beiden Bildschirmrichtungen. Sprache und Emotes halten den Lauf an. Ohne geeignete Laufanimation beziehungsweise unterstütztes Geh-Rig bleibt sie stehen.

Nach mehreren sichtbaren Wegpunkten darf sie seitlich ganz aus dem Bild laufen, drei bis acht aktive Sekunden außerhalb bleiben und anschließend wieder hereinkommen. Eine anstehende Sprachausgabe oder ausdrücklich ausgelöste Geste hat Vorrang: Sobald ihr Audio vorbereitet ist, läuft sie zum nächsten vollständig sichtbaren Punkt zurück und kommt zum Stehen. Erst ein bestätigter Darstellungsframe gibt Stimme, Untertitel, Emote und dessen Sound frei. Textausgaben bei ausgeschalteter Stimme warten ebenfalls auf diese Rückkehr. Die Figur bleibt für laufende Sprache, Gesten und deren Soundende sichtbar. Kleine zufällige Gesten außerhalb des Bildes werden übersprungen.

Stop, Profil-/Assetwechsel und das Schließen der Ausgabe verwerfen eine wartende Präsentation; eine verspätete Rückkehr startet keine abgebrochene Antwort. Wird die Figur nicht innerhalb von 20 Sekunden darstellungsbereit, fällt die Ausgabe mit einem Hinweis aus, statt unsichtbar zu sprechen. Für die Seiten-Ausflüge wird das native Fenster nicht über den gewählten Monitor hinaus erweitert. Studio und OBS behalten ihre begrenzte Bühne.

Unter **Ausgabemonitor für den Autopiloten** lässt sich ein Monitor auswählen. Die automatische Auswahl verwendet den Monitor mit der größten Überschneidung des festen Fensters; die Liste wird beim Fokussieren des Studios oder Öffnen der Auswahl aktualisiert. Das ist eine Ausgabeentscheidung und **keine Freigabe für Bildschirmaufnahmen**. Jeder Bot hat seine eigene Ausgabe und Sitzung.

Während der Wanderung ist das gesamte Fenster durchklickbar und der Ziehgriff ausgeblendet. **Autopilot pausieren**, **Stop** im Studio, der Tray-Stop oder der Control-MCP stellen die vorherige feste Position und Größe wieder her. Die gespeicherte Klickdurchleitung gilt anschließend wieder. Fensterbewegungen während der Wanderung überschreiben die feste Position nicht. Nach einem App-Neustart bleibt der Autopilot aus.

**Figurgröße live ändern** wirkt sofort in Studio, nativer Ausgabe und verbundenen OBS-Quellen, einschließlich neu geöffneter Ausgaben. Der Regler ändert die Figur, nicht die abgedeckte Monitorfläche. Die Vorschau speichert ausschließlich diesen Laufzeitwert; andere Entwürfe oder Berechtigungen werden nicht mitgespeichert. **Änderungen speichern** übernimmt ihn dauerhaft. Dasselbe gilt für die Monitorauswahl. Die Desktopkamera passt sehr große Figuren an die Fläche an, ohne die normale Kameraeinstellung zu ändern. Bei sichtbaren Auftritten wird die ganze Figur berücksichtigt; seitliche Autopilot-Ausflüge verlassen absichtlich das Bild. Untertitel bleiben nahe der Figur und innerhalb des Bildschirms.

Fensteränderungen können vom Betriebssystem verzögert oder zusammengefasst werden. Die App behält die Klickdurchleitung und die feste Geometrie, bis feldbezogene Änderungsbelege und tatsächliche Fensterwerte die Rückstellung bestätigen. Fehlen diese Belege nach begrenzten Wiederholungen, wird das alte weiterhin durchklickbare Fenster geschlossen und an der gespeicherten festen Stelle neu geöffnet. Eine dabei laufende Sprachausgabe wird abgebrochen; sie wird nicht erneut abgespielt. Eine inzwischen geschlossene oder umgestellte Ausgabe wird nicht wieder geöffnet.

Vorhandene geeignete Laufanimationen haben Vorrang. Die automatische Gehbewegung verwendet keine inverse Kinematik für eine exakte Fußbindung an den Boden. Ungewöhnliche fremde Skelette und nicht standardisierte Vorwärtsachsen brauchen eine separate Kompatibilitätsprüfung. Nova und unterstützte humanoide VRM-Skelette erhalten einen automatischen Gehzyklus. Bei unbekannten Skeletten ohne geeigneten Clip wird kein scheinbar unterstütztes Gehen vorgetäuscht. Manuelle Emotes, Sprache und reduzierte Bewegung im Betriebssystem haben Vorrang. Kleine Bewegungen ohne Sprache sind stumm; ausdrücklich konfigurierte Emotes bei einer gesprochenen Antwort behalten ihre normalen Soundregeln.

Die Bewegungszeitlinie verwendet den Sitzungsbeginn und denselben Zufallsstartwert in Studio, Overlay und OBS. Verschiedene Kameras und Fenstergrößen können zu unterschiedlichen begrenzten Laufwegen führen. Die [Formatübersicht](AVATAR-SUPPORT.md) erläutert unterstützte Modellfunktionen und Importgrenzen.

## Eigene Beiträge und Kosten

Standardmäßig wartet der Bot nach Gesprächsaktivität zufällig **60 bis 180 Sekunden**. Er darf höchstens **zwölf Versuche in einer rollenden Stunde** unternehmen. Ein Versuch kann einen Kommentar, eine Frage oder bewusstes Schweigen ergeben. Fehler und Schweigen zählen ebenfalls zum Budget. Stoppen und erneutes Starten setzen das Stundenbudget während derselben App-Laufzeit nicht zurück.

Auch ohne neue Chatnachricht oder Bildschirmaufnahme darf der Bot mit einer kurzen, unverfänglichen Frage oder Bemerkung aus seiner Persona ein Gespräch beginnen. Aussagen über konkrete Ereignisse, Bildschirm-, Spiel- oder Zuschauerinhalte benötigen weiterhin tatsächlich vorliegende Informationen; der Bot darf Wahrnehmungen nicht erfinden und kann sich bewusst fürs Schweigen entscheiden.

Gespräche, laufende Sprachausgaben und Mikrofonaufnahmen haben Vorrang. Offene Änderungen, Eingabeentwürfe und Update-Dialoge verschieben neue spontane Beiträge. Versäumte Beiträge werden nicht nach einer Pause gesammelt nachgeholt. Mehrere Bots sprechen nacheinander, mit einer erneuten Ruhepause nach dem tatsächlichen Ausgabeende. Eine laufende Antwort lässt sich mit Stop abbrechen.

### Wenn die Figur läuft, aber nichts sagt

Bewegung und Gesten sind von Sprechversuchen unabhängig. Im aktiven Autopiloten zeigt **Sprechstatus** den tatsächlichen Wartegrund oder den Countdown zum nächsten **Versuch**. Bei **Wartet: Änderungen speichern** die Profil- und Anweisungsänderungen speichern; dazu zählen auch eine nur vorübergehend geänderte Figurgröße oder neue Autopilot-Abstände. Einen Nachrichtenentwurf senden oder leeren und den Update- oder Erweiterungsdialog schließen. Nach dem Ende einer solchen Aktivität beginnt wieder die vollständige zufällige Ruhepause. Bei einem selbst eingestellten Abstand von 120 bis 180 Sekunden sind das erneut zwei bis drei Minuten.

Auch ausgeschaltetes Sprechen, ein fehlendes Dialogmodell, eine inaktive Ausgabe, laufende Antworten und das ausgeschöpfte Stundenbudget werden angezeigt. Der Countdown garantiert keine hörbare Antwort: Modellberechnung und Stimme brauchen anschließend Zeit, und das Modell darf keinen Beitrag liefern. Ein solcher stummer Versuch erscheint im Sprechstatus und im Aktivitätsprotokoll; er zählt wie bisher zum Stundenbudget. Die reine Statusanzeige erstellt keine Modellanfrage und verbraucht kein Budget.

Bewegungen lösen keine KI-Anfragen aus. Eigene Beiträge verwenden den gewählten Anbieter und können dessen übliche Kosten für Dialog, Stimme und Hindsight verursachen. Die Intervalle sind zwischen 15 und 3600 Sekunden und das Stundenbudget zwischen 1 und 120 Versuchen einstellbar. Das Budget begrenzt die Anzahl der Gesprächsversuche, keinen Geldbetrag und keine internen Werkzeugrunden.

Text, Stimme und eingeplante Emotes verwenden denselben Wiedergabeablauf wie normale Antworten. Ein stummer Versuch erzeugt keine künstliche Nutzernachricht oder Sprachausgabe. Autopilot-Beiträge werden nicht automatisch in Twitch oder YouTube gepostet.

## Optionaler Bildschirmkontext

Unter **Bildschirm als Kontext** lässt sich die eingebaute Bildschirmaufnahme dieses Bots einschalten. Dafür sind weder MCP-Server noch Skills oder Werkzeugfreigaben nötig. Ein bildfähiges Modell, die allgemeine Bildfreigabe und passende Betriebssystemrechte werden benötigt. Dieselbe Einstellung steht unter **Modelle** und gilt auch für normale Gespräche.

Unmittelbar vor jedem Modellaufruf nimmt die App einen frischen Screenshot des Hauptbildschirms oder des ausdrücklich ausgewählten Monitors auf. Auch Werkzeug-Folgerunden bekommen ein neues Bild; frühere Bildschirmbilder werden nicht angesammelt. Bilder gehen an den gewählten Anbieter und verlassen bei Cloud-/Remote-Modellen den Rechner. Es wird kein fortlaufendes Bildschirmvideo an das Modell übertragen und kein Screenshot im Gesprächsverlauf oder in Hindsight gespeichert.

Bei fehlender Freigabe, verschwundenem Monitor oder Aufnahmefehler wird die Anfrage mit einer konkreten Fehlermeldung abgebrochen. Allgemeine MCP-Steuerung und Skill-Skripte bleiben im Autopiloten gesperrt. Sichtbare Texte können keine neuen Rechte vergeben. Details zu Einrichtung, Datenschutz, Limits und der ausdrücklichen Wayland-Quellenauswahl und ausstehenden Desktop-Abnahme stehen unter [Bildschirm als Kontext](SCREEN-CONTEXT.md).

## Vorbild und Grenzen

Die gewünschte Wirkung orientiert sich an den selbst begonnenen Auftritten von Frankie in [DougDougDougs Frosch-Video](https://www.youtube.com/watch?v=0UjMRq3UHH0), insbesondere um 1:35 und 3:40. Die sichtbaren Auftritte belegen nicht, wie die dortige Software intern implementiert ist. Diese App verwendet ihre eigene 3D-Darstellung und Anbieteranbindung; die Froschfigur und Videoassets wurden nicht übernommen.

Die Desktop-App muss weiterlaufen. Das native Ausgabefenster bleibt auf dem gewählten Monitor; seitlich auslaufende Figuren werden außerhalb dieses Fensters abgeschnitten. Monitorgröße und DPI beeinflussen die Darstellung; das Render-Pixelverhältnis bleibt auf 1,75 und die vorhandene Bildrate begrenzt. Die Begrenzung verwendet die gemessenen neutralen Modellgrenzen und eine Drehhülle; bekannte Rootbewegungen wie Novas Freudensprung werden am Rand korrigiert. Ungewöhnliche eigene Skelettanimationen, die weit über diese Modellgrenzen hinausreichen, können eine separate Assetprüfung brauchen. Absichtliches Verstecken darf die Fläche verlassen.

Die Planung spontaner Beiträge läuft derzeit im Studio-WebView. Auf **macOS 14 und neuer** deaktiviert dessen Konfiguration die WebKit-Hintergrunddrosselung, damit das Verstecken des Studios über das Tray die Planung nicht absichtlich suspendiert. Diese Tauri-Einstellung wird unter älteren macOS-Versionen sowie Windows und Linux nicht unterstützt. Dort können Betriebssystem und WebView versteckte oder minimierte Fenster weiterhin drosseln; bei ausbleibenden Beiträgen das Studio sichtbar lassen. Das ist keine Zusicherung bei Ruhezustand, gesperrtem Rechner oder beendeter App. Eine vollständig native Sprachplanung bleibt ein gesonderter Ausbau. Das Hintergrundverhalten wurde nicht mit echten versteckten App-Fenstern interaktiv abgenommen.

Die automatischen Prüfungen verwenden echte Three.js-Kameras und Modellgrenzen sowie simulierte native Fensterereignisse und DPI-Wechsel. Echte Desktopfenster, Multi-Monitor-Wechsel und Betriebssystem-Compositoren wurden für diese Änderung nicht interaktiv abgenommen. Insbesondere entscheidet ein Wayland-Compositor selbst über native Positionierung und durchklickbare Fenster; fehlende Bestätigungen dürfen nicht als erfolgreicher Layoutwechsel ausgegeben werden. Selbst ausgeführte OBS-/Computeraktionen benötigen weiterhin einen eigenen ausdrücklich freigegebenen Steuerungsablauf.
