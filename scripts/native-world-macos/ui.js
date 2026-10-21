// osascript JXA: native System Events / ObjC only. Never WebView JS or IPC.
// All output is closed numeric metadata; native exceptions/UI text are withheld.
const APP_ID = 'studio.orbitpal.native-world-fixture';
const TITLE = 'OrbitPal Native World Fixture';
const SYSTEM_EVENTS_PATH = '/System/Library/CoreServices/System Events.app';
const OPERATIONS = ['probe', 'inspect', 'autopilot', 'setup', 'nova', 'blue', 'start', 'hide', 'show', 'stop', 'quit'];
function closed(code, extra) {
  return Object.assign({version: 1, code, ax: 0, appleEvents: 0, gui: 0, probePhase: 0, aeFunction: 0, aeStatusKind: 4, targetRunning: 0, screenPresent: 0, sessionPresent: 0, uidMatch: 0, onConsole: 0, loginDone: 0, lockKnown: 0, lockState: 0, matched: 0, acted: 0, mainVisible: 0, windows: 0}, extra || {});
}
function aeStatusKind(status) {
  // Public AppleEvents.h / MacErrors.h constants; never export a raw OSStatus.
  return status === 0 ? 0 : status === -1744 ? 1 : status === -1743 ? 2 : status === -600 ? 3 : 4;
}
function one(values) { if (!values.length) throw 9; if (values.length !== 1) throw 5; return values[0]; }
function inside(child, parent) {
  return child[0] > 0 && child[1] > 0 && child[2] >= parent[2] && child[3] >= parent[3]
    && child[2] + child[0] <= parent[2] + parent[0] && child[3] + child[1] <= parent[3] + parent[1];
}
function matchesButton(node, label, actor, walk) {
  if (node.role() !== 'AXButton') return false;
  if (actor) {
    // An empty label is never an alternate actor selector. Require the exact
    // seeded actor's actual native static text inside this button.
    return ['Fixture Nova', 'Fixture Blue'].includes(actor) && walk(node).some(child =>
      child.role() === 'AXStaticText' && (child.name() === actor || child.value() === actor));
  }
  return ['Autopilot', 'Set up desktop', 'Save and start Life', 'Stop Life'].includes(label)
    && node.name() === label;
}
function run(argv) {
  let out = closed(1);
  try {
    if (argv.length !== 4 || !OPERATIONS.includes(argv[0])) throw 1;
    ObjC.import('AppKit'); ObjC.import('ApplicationServices'); ObjC.import('CoreServices');
    out.ax = $.AXIsProcessTrusted() ? 1 : 0;
    if (!out.ax) throw 2; // No prompt option, TCC write or permission request.
    function systemEventsProcess() {
      const apps = $.NSRunningApplication.runningApplicationsWithBundleIdentifier('com.apple.systemevents');
      const count = Number(apps.count);
      if (count === 0) return null;
      if (count !== 1) throw 5;
      const app = apps.objectAtIndex(0), pid = Number(app.processIdentifier);
      if (!app || app.isTerminated || !Number.isSafeInteger(pid) || pid < 1 || pid > 2147483647
        || ObjC.unwrap(app.bundleIdentifier) !== 'com.apple.systemevents'
        || ObjC.unwrap(app.bundleURL.path) !== SYSTEM_EVENTS_PATH
        || ObjC.unwrap(app.executableURL.path) !== SYSTEM_EVENTS_PATH + '/Contents/MacOS/System Events') throw 5;
      return {app, pid};
    }
    out.probePhase = 7; // Fixed Apple target identity, before any permission query.
    let systemEvents = null;
    for (let attempt = 0; attempt < 20; attempt++) {
      systemEvents = systemEventsProcess();
      if (systemEvents) break;
      if (attempt < 19) $.NSThread.sleepForTimeInterval(0.05);
    }
    if (!systemEvents) throw 5;
    out.targetRunning = 1;
    out.probePhase = 1; // Explicit C binding; umbrella metadata may omit AE.
    // OSStatus / const AEAddressDesc* / AEEventClass / AEEventID / Boolean.
    // The real descriptor owns its pointer throughout this synchronous call.
    ObjC.bindFunction('AEDeterminePermissionToAutomateTarget', ['int', ['void *', 'unsigned int', 'unsigned int', 'bool']]);
    out.aeFunction = typeof $.AEDeterminePermissionToAutomateTarget === 'function' ? 1 : 0;
    if (!out.aeFunction) throw 8;
    out.probePhase = 2; // Native application-address descriptor factory.
    const target = $.NSAppleEventDescriptor.descriptorWithBundleIdentifier('com.apple.systemevents');
    out.probePhase = 3; // Native retained descriptor's pointer getter.
    const address = target.aeDesc;
    if (!address) throw 8;
    out.probePhase = 7;
    out.targetRunning = 0;
    const currentSystemEvents = systemEventsProcess();
    if (!currentSystemEvents || currentSystemEvents.pid !== systemEvents.pid || systemEvents.app.isTerminated) throw 5;
    out.targetRunning = 1;
    out.probePhase = 4; // Actual permission query, never ask for consent.
    // Public nonprompting permission API, evaluated by the actual osascript client.
    const permission = $.AEDeterminePermissionToAutomateTarget(address, 0x2a2a2a2a, 0x2a2a2a2a, false);
    if (!Number.isInteger(permission)) throw 8;
    out.aeStatusKind = aeStatusKind(permission);
    out.appleEvents = permission === 0 ? 1 : 0;
    if (!out.appleEvents) throw 3;
    out.probePhase = 5; // Actual same-UID, console, login and display checks.
    ObjC.bindFunction('getuid', ['unsigned int', []]);
    ObjC.import('CoreFoundation');
    // CFDictionaryRef is toll-free bridged to NSDictionary. Its opaque imported
    // C return encoding is not an ObjC wrapper for deepUnwrap; bind the same
    // pointer ABI as id, with no additional session sample or value coercion.
    ObjC.bindFunction('CGSessionCopyCurrentDictionary', ['id', []]);
    const sessionDictionary = $.CGSessionCopyCurrentDictionary();
    let session;
    if (!sessionDictionary || sessionDictionary.isNil()) {
      out.sessionPresent = 1;
      throw 4;
    }
    try {
      if (sessionDictionary.isKindOfClass($.NSDictionary) !== true) throw 4;
      session = ObjC.deepUnwrap(sessionDictionary);
    } finally {
      // Balance the native Copy result's +1 exactly once, after conversion.
      // The JS primitive copy survives; no borrowed child receives CFRelease.
      $.CFRelease(sessionDictionary);
    }
    const lockKnown = session && Object.prototype.hasOwnProperty.call(session, 'CGSSessionScreenIsLocked');
    const screenCount = Number($.NSScreen.screens.count);
    // Preserve the original native call's short circuit: getuid runs only when
    // the screen/session prerequisites were already positive. No extra sample.
    const processUid = screenCount > 0 && session ? Number($.getuid()) : undefined;
    out.gui = screenCount > 0 && session
      && session.kCGSSessionUserIDKey === processUid
      && session.kCGSSessionOnConsoleKey === true && session.kCGSessionLoginDoneKey === true
      && (!lockKnown || session.CGSSessionScreenIsLocked === false) ? 1 : 0;
    // Diagnostics only: 0 unknown/unread/wrong type, 1 false, 2 true. Never
    // normalize a bridged number/string to satisfy the unchanged strict gate.
    const booleanState = value => typeof value === 'boolean' ? value ? 2 : 1 : 0;
    out.screenPresent = Number.isFinite(screenCount) ? screenCount > 0 ? 2 : 1 : 0;
    out.sessionPresent = session == null ? 1 : typeof session === 'object' && !Array.isArray(session) ? 2 : 0;
    if (session) {
      out.uidMatch = Number.isSafeInteger(session.kCGSSessionUserIDKey) && Number.isSafeInteger(processUid)
        ? session.kCGSSessionUserIDKey === processUid ? 2 : 1 : 0;
      out.onConsole = booleanState(session.kCGSSessionOnConsoleKey);
      out.loginDone = booleanState(session.kCGSessionLoginDoneKey);
      out.lockKnown = lockKnown ? 2 : 1;
      out.lockState = lockKnown ? booleanState(session.CGSSessionScreenIsLocked) : 0;
    }
    if (!out.gui) throw 4;
    out.probePhase = 6; // Every native prerequisite above completed positively.
    if (argv[0] === 'probe') return JSON.stringify(Object.assign(out, {code: 0}));
    const pid = Number(argv[1]);
    if (!Number.isSafeInteger(pid) || pid < 1 || pid > 2147483647 || !argv[2].startsWith('/') || !argv[3].startsWith('/')) throw 1;
    const native = $.NSRunningApplication.runningApplicationWithProcessIdentifier(pid);
    if (!native || native.isTerminated || ObjC.unwrap(native.bundleIdentifier) !== APP_ID
      || ObjC.unwrap(native.bundleURL.path) !== argv[2] || ObjC.unwrap(native.executableURL.path) !== argv[3]) throw 5;
    const se = Application('com.apple.systemevents');
    function process() { return one(se.applicationProcesses.whose({unixId: pid})()); }
    function windows() { return process().windows(); }
    function main() { return one(windows().filter(w => w.name() === TITLE)); }
    function frame(node) { const p = node.position(), s = node.size(); return [s[0], s[1], p[0], p[1]]; }
    function walk(root) {
      const queue = [root], result = [];
      while (queue.length && result.length < 1200) {
        const node = queue.shift(); result.push(node);
        const available = 1200 - result.length - queue.length;
        if (available > 0) queue.push(...node.uiElements().slice(0, available));
      }
      return result;
    }
    function text(node) {
      const name = node.name();
      return typeof name === 'string' ? name : '';
    }
    function button(label, actor) {
      const current = main();
      const matches = walk(current).filter(node => matchesButton(node, label, actor, walk));
      const node = one(matches);
      out.matched = 1;
      if (node.enabled() !== true || !inside(frame(node), frame(current))) throw 6;
      return node;
    }
    function click(node) {
      // Native action on this exact fresh AX object. No keyboard, pointer or eval.
      se.click(node); out.acted = 1;
    }
    function tray(item) {
      // Only this owned app's second (status-item) menu bar, never another app/Dock.
      const bars = process().menuBars();
      if (bars.length !== 2) throw 7;
      click(one(bars[1].menuBarItems()));
      const menu = one(one(process().menuBars()[1].menuBarItems()).menus());
      const items = menu.menuItems();
      if (items.filter(i => text(i) === 'Show studio').length !== 1 || items.filter(i => text(i) === 'Quit OrbitPal').length !== 1) throw 7;
      const selected = one(items.filter(i => text(i) === item));
      if (selected.enabled() !== true) throw 7;
      click(selected);
    }
    out.windows = Math.min(windows().length, 16);
    out.mainVisible = windows().filter(w => w.name() === TITLE).length === 1 ? 1 : 0;
    if (argv[0] === 'inspect') {
      button('Autopilot'); // Actual Main WebView AX before any Life action.
      const bars = process().menuBars();
      if (bars.length !== 2) throw 7;
      one(bars[1].menuBarItems()); // A native recovery/normal-Quit route must exist.
    } else if (argv[0] === 'show') {
      tray('Show studio');
    } else if (argv[0] === 'quit') {
      tray('Quit OrbitPal');
    } else {
      process().frontmost = true; // Disposable VM only, exact retained process.
      const current = main();
      if (process().frontmost() !== true) throw 6;
      const labels = {autopilot: 'Autopilot', setup: 'Set up desktop', start: 'Save and start Life', stop: 'Stop Life'};
      if (argv[0] === 'hide') {
        click(one(current.buttons().filter(node => node.subrole() === 'AXCloseButton')));
      } else {
        const actor = argv[0] === 'nova' ? 'Fixture Nova' : argv[0] === 'blue' ? 'Fixture Blue' : null;
        click(button(labels[argv[0]] || '', actor));
      }
    }
    out.code = 0;
  } catch (error) {
    out.code = Number.isInteger(error) && error >= 1 && error <= 9 ? error : 8;
  }
  return JSON.stringify(out);
}
if (typeof module !== 'undefined') module.exports = {closed, one, inside, matchesButton, OPERATIONS, run};
