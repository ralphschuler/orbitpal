// osascript JXA: native System Events / ObjC only. Never WebView JS or IPC.
// All output is closed numeric metadata; native exceptions/UI text are withheld.
const APP_ID = 'studio.orbitpal.native-world-fixture';
const TITLE = 'OrbitPal Native World Fixture';
const OPERATIONS = ['probe', 'inspect', 'autopilot', 'setup', 'nova', 'blue', 'start', 'hide', 'show', 'stop', 'quit'];
function closed(code, extra) {
  return Object.assign({version: 1, code, ax: 0, appleEvents: 0, gui: 0, matched: 0, acted: 0, mainVisible: 0, windows: 0}, extra || {});
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
    const target = $.NSAppleEventDescriptor.descriptorWithBundleIdentifier('com.apple.systemevents');
    // Public nonprompting permission API, evaluated by the actual osascript client.
    out.appleEvents = $.AEDeterminePermissionToAutomateTarget(target.aeDesc, 0x2a2a2a2a, 0x2a2a2a2a, false) === 0 ? 1 : 0;
    if (!out.appleEvents) throw 3;
    ObjC.bindFunction('getuid', ['unsigned int', []]);
    const session = ObjC.deepUnwrap($.CGSessionCopyCurrentDictionary());
    const lockKnown = session && Object.prototype.hasOwnProperty.call(session, 'CGSSessionScreenIsLocked');
    out.gui = Number($.NSScreen.screens.count) > 0 && session
      && session.kCGSSessionUserIDKey === Number($.getuid())
      && session.kCGSSessionOnConsoleKey === true && session.kCGSessionLoginDoneKey === true
      && (!lockKnown || session.CGSSessionScreenIsLocked === false) ? 1 : 0;
    if (!out.gui) throw 4;
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
if (typeof module !== 'undefined') module.exports = {closed, one, inside, matchesButton, OPERATIONS};
