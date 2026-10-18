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

Die Figur bewegt sich mit Ruhepausen innerhalb ihrer vorhandenen Bühne. Für ein größeres Spielfeld kann der Streamer die OBS-Browserquelle oder das transparente Figurenfenster selbst vergrößern und platzieren. Der Autopilot verschiebt keine Betriebssystemfenster. Für sichtbare Schritte ist **Ganze Figur** unter **Charakter & Stimme** sinnvoll.

Vorhandene geeignete Laufanimationen haben Vorrang. Die automatische Gehbewegung verwendet keine inverse Kinematik für eine exakte Fußbindung an den Boden. Ungewöhnliche fremde Skelette und nicht standardisierte Vorwärtsachsen brauchen eine separate Kompatibilitätsprüfung. Nova und unterstützte humanoide VRM-Skelette erhalten einen automatischen Gehzyklus. Bei unbekannten Skeletten ohne geeigneten Clip wird kein scheinbar unterstütztes Gehen vorgetäuscht. Manuelle Emotes, Sprache und reduzierte Bewegung im Betriebssystem haben Vorrang. Kleine Bewegungen ohne Sprache sind stumm; ausdrücklich konfigurierte Emotes bei einer gesprochenen Antwort behalten ihre normalen Soundregeln.

Die Bewegungszeitlinie verwendet den Sitzungsbeginn und denselben Zufallsstartwert in Studio, Overlay und OBS. Verschiedene Kameras und Fenstergrößen können zu unterschiedlichen begrenzten Laufwegen führen. Die [Formatübersicht](AVATAR-SUPPORT.md) erläutert unterstützte Modellfunktionen und Importgrenzen.

## Eigene Beiträge und Kosten

Standardmäßig wartet der Bot nach Gesprächsaktivität zufällig **60 bis 180 Sekunden**. Er darf höchstens **zwölf Versuche in einer rollenden Stunde** unternehmen. Ein Versuch kann einen Kommentar, eine Frage oder bewusstes Schweigen ergeben. Fehler und Schweigen zählen ebenfalls zum Budget. Stoppen und erneutes Starten setzen das Stundenbudget während derselben App-Laufzeit nicht zurück.

Gespräche, laufende Sprachausgaben und Mikrofonaufnahmen haben Vorrang. Offene Änderungen, Eingabeentwürfe und Update-Dialoge verschieben neue spontane Beiträge. Versäumte Beiträge werden nicht nach einer Pause gesammelt nachgeholt. Mehrere Bots sprechen nacheinander, mit einer erneuten Ruhepause nach dem tatsächlichen Ausgabeende. Eine laufende Antwort lässt sich mit Stop abbrechen.

Bewegungen lösen keine KI-Anfragen aus. Eigene Beiträge verwenden den gewählten Anbieter und können dessen übliche Kosten für Dialog, Stimme und Hindsight verursachen. Die Intervalle sind zwischen 15 und 3600 Sekunden und das Stundenbudget zwischen 1 und 120 Versuchen einstellbar. Das Budget begrenzt die Anzahl der Gesprächsversuche, keinen Geldbetrag und keine internen Werkzeugrunden.

Text, Stimme und eingeplante Emotes verwenden denselben Wiedergabeablauf wie normale Antworten. Ein stummer Versuch erzeugt keine künstliche Nutzernachricht oder Sprachausgabe. Autopilot-Beiträge werden nicht automatisch in Twitch oder YouTube gepostet.

## Optionaler Bildschirmkontext

Bildschirmkontext bleibt zunächst ausgeschaltet. Für die Nutzung müssen ein MCP-Server installiert und diesem Bot passende Werkzeuge freigegeben sein. Zusätzlich braucht das gewählte Dialogmodell Bildverarbeitung sowie die aktivierten Werkzeug- und Bildfreigaben.

Unter **Bildschirm als Kontext** einen Server und genau ein **reines Beobachtungswerkzeug** wählen. **Werkzeuge laden** verbindet den ausgewählten Server und zeigt dessen für den Bot freigegebene Werkzeuge; dabei wird noch kein Screenshot angefordert. Werkzeugname und JSON-Argumente werden gespeichert. Die Argumente müssen zur konkreten MCP-Implementierung passen, etwa zur Auswahl eines Fensters und zur Rückgabe eines Bildes. Ein bloßer Dateipfad oder Text mit einer Bildreferenz ist keine Bildausgabe.

Vor einem spontanen Modellbeitrag ruft die App dieses fest konfigurierte Werkzeug einmal auf. Das Modell kann weder den Werkzeugnamen noch diese Argumente austauschen und erhält im Autopiloten keine allgemeine MCP-Steuerung oder Skill-Skriptausführung. Ein MCP-Server läuft dennoch mit seinen eigenen Betriebssystemrechten: Der Betreiber muss tatsächlich ein Beobachtungswerkzeug auswählen, kein Werkzeug mit verändernder Wirkung.

Echte Werkzeugbilder werden durch die bestehende Bildverarbeitung an den Dialoganbieter weitergegeben. Bei Cloud-Anbietern verlassen sie damit den Rechner; ein lokales Dialogmodell hält die Modellverarbeitung lokal, sofern auch die beteiligten MCP-, Sprach- und Gedächtnisdienste lokal laufen. Es gibt keinen fortlaufenden Bildschirmvideostream. Ohne aktive Sitzung und fälligen Gesprächsversuch fordert der Autopilot keine Beobachtung an.

Fehlgeschlagene Beobachtungen erscheinen im Aktivitätsprotokoll. Der Bot erhält dann die Information, dass kein Bildschirmkontext verfügbar ist, und soll ohne Bildschirmbezug sprechen oder schweigen. Sichtbare Texte, Chat, Dokumente und Werkzeugantworten bleiben unzuverlässige Inhalte und können keine neuen Rechte vergeben.

## Vorbild und Grenzen

Die gewünschte Wirkung orientiert sich an den selbst begonnenen Auftritten von Frankie in [DougDougDougs Frosch-Video](https://www.youtube.com/watch?v=0UjMRq3UHH0), insbesondere um 1:35 und 3:40. Die sichtbaren Auftritte belegen nicht, wie die dortige Software intern implementiert ist. Diese App verwendet ihre eigene 3D-Darstellung und Anbieteranbindung; die Froschfigur und Videoassets wurden nicht übernommen.

Die Desktop-App muss weiterlaufen. Für eine Figur, die außerhalb ihrer Bühne über den gesamten Desktop wandert, oder für selbst ausgeführte OBS-/Computeraktionen wäre ein zusätzlicher ausdrücklich freigegebener Steuerungsablauf nötig.
