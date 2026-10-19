# Mikrofonfreigabe nach einem Update

Die Aufnahme und die Erkennung einer Aktivierungsphrase starten ausschließlich mit deinem Klick. Ein gespeichertes Profil, eine erneute Statusprüfung und das Zurückkehren aus den Systemeinstellungen starten keine Aufnahme.

## macOS

Öffne **Systemeinstellungen → Datenschutz & Sicherheit → Mikrofon**, aktiviere **OrbitPal** und beende und öffne die App nach einer geänderten Freigabe erneut. Klicke anschließend selbst auf den Aufnahme- oder Startknopf. Im Regiepult zeigt **Mikrofonfreigabe** den nativen macOS-Status und bietet einen Knopf zum Öffnen dieser Einstellungen. Die Statusprüfung aktiviert kein Mikrofon. Die WebView muss die Aufnahme zusätzlich erlauben; eine erteilte macOS-Freigabe ist deshalb noch kein Nachweis einer erfolgreichen Aufnahme.

Fehlt die App in der Liste, installiere die korrigierte Version und klicke zuerst auf Aufnahme oder **Zuhören starten**, damit die Systemabfrage erscheint. Bei einer durch Bildschirmzeit oder Geräteverwaltung eingeschränkten Freigabe muss die entsprechende Systemregel geändert werden.

In den macOS-Paketen von v0.7.0 fehlte das Entitlement `com.apple.security.device.audio-input`, obwohl der Hardened Runtime aktiv war. Die vorhandene `NSMicrophoneUsageDescription` allein genügt nicht. Korrigierte Pakete signieren das erforderliche Audio-Input-Entitlement; die Release-Pipeline prüft sowohl die signierte `.app` als auch die eingebundene Beschreibung. Ein reines Umschalten der Systemeinstellung kann das fehlende Entitlement in v0.7.0 nicht beheben.

Die bisherigen macOS-Pakete sind ad hoc signiert. Ihre vom Code-Hash abhängige Identität kann sich bei Updates ändern; eine dauerhaft stabile Developer-ID-Signatur bleibt die erforderliche Verbesserung für verlässliche Identität zwischen Veröffentlichungen. Die App setzt keine Systemfreigaben zurück und lockert ihre Signaturanforderungen nicht.

## Windows und Linux

Die WebView fragt beim expliziten Start nach der Mikrofonfreigabe. Unter Windows muss außerdem der Mikrofonzugriff für Desktop-Apps erlaubt sein. Der Knopf **Mikrofoneinstellungen öffnen** führt direkt dorthin. Unter Linux prüfe die Audioeingabe und Datenschutzregeln deiner Desktop-Umgebung. Bei fehlendem Gerät, abgelehnter Freigabe oder einer veralteten WebView wird kein erfolgreicher Mikrofonstart vorgetäuscht.

## Technische Quellen

- Apple: [Audio Input Entitlement](https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.security.device.audio-input)
- Apple: [Requesting authorization to capture and save media](https://developer.apple.com/documentation/avfoundation/requesting-authorization-to-capture-and-save-media)
- Apple: [AVCaptureDevice authorizationStatus](https://developer.apple.com/documentation/avfoundation/avcapturedevice/authorizationstatus(for:))
