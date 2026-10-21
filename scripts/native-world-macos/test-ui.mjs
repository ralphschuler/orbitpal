// Execute production predicates and the probe in a Node VM with controlled
// ObjC/Application doubles only. No native UI or permission operation runs.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
const context = {module: {exports: {}}};
const source = readFileSync(new URL('./ui.js', import.meta.url), 'utf8');
new vm.Script(source).runInNewContext(context);
const {matchesButton, one, nativeTree} = context.module.exports;
const node = (role, name, children = [], value = name) => ({role: () => role, name: () => name, value: () => value, children});
const walk = root => [root, ...root.children];
const match = (value, actor) => matchesButton(value, '', actor, walk);
const blank = node('AXButton', '');
assert.equal(match(blank, 'Fixture Nova'), false);
assert.equal(match(blank, 'Fixture Blue'), false);
assert.equal(matchesButton(blank, '', null, walk), false);
const wrong = node('AXButton', '', [node('AXStaticText', 'Fixture Blue')]);
assert.equal(match(wrong, 'Fixture Nova'), false);
assert.equal(match(wrong, 'Fixture Blue'), true);
const nova = node('AXButton', '', [node('AXStaticText', 'Fixture Nova')]);
assert.equal(match(nova, 'Fixture Nova'), true);
assert.equal(match(nova, 'Fixture Blue'), false);
assert.equal(match(node('AXButton', 'Fixture Nova'), 'Fixture Nova'), false);
assert.equal(match(node('AXGroup', '', [node('AXStaticText', 'Fixture Nova')]), 'Fixture Nova'), false);
assert.equal(match(node('AXButton', '', [node('AXStaticText', 'Fixture Nova PRIVATE')]), 'Fixture Nova'), false);
assert.equal(match(node('AXButton', '', [node('AXStaticText', '', [], 'Fixture Nova')]), 'Fixture Nova'), true);
assert.equal(match(nova, 'PRIVATE_ACTOR'), false);
const buttons = [blank, wrong, nova];
assert.equal(one(buttons.filter(value => match(value, 'Fixture Nova'))), nova);
assert.throws(() => one([nova, node('AXButton', '', [node('AXStaticText', 'Fixture Nova')])]
  .filter(value => match(value, 'Fixture Nova'))));
assert.equal(matchesButton(node('AXButton', 'Autopilot'), 'Autopilot', null, walk), true);
assert.equal(matchesButton(node('AXButton', 'Autopilot PRIVATE'), 'Autopilot', null, walk), false);
assert.equal(matchesButton(node('AXButton', ''), '', null, walk), false);
assert.equal(matchesButton(node('AXButton', 'PRIVATE_LABEL'), 'PRIVATE_LABEL', null, walk), false);
console.log('native_world_macos_ui_predicate_passed');

function probe(options = {}) {
  const calls = [], bindings = [], pointer = {}, target = {};
  const dictionaryClass = {}, sessionDictionary = {
    isNil: () => options.nilSession === true,
    isKindOfClass(value) { assert.equal(value, dictionaryClass); fail('class'); return options.foreignSession !== true; },
  };
  let applicationCalls = 0, targetReads = 0, sleeps = 0;
  const fail = stage => { if (options.throwAt === stage) throw new Error('PRIVATE_NATIVE_ERROR'); };
  Object.defineProperty(target, 'aeDesc', {get() {
    calls.push('descriptor'); fail('descriptor'); return options.nilPointer ? null : pointer;
  }});
  const $ = {
    AXIsProcessTrusted: () => options.ax !== false,
    NSRunningApplication: {runningApplicationWithProcessIdentifier(pid) {
      assert.equal(pid, 42);
      return {isTerminated: false, bundleIdentifier: 'studio.orbitpal.native-world-fixture',
        bundleURL: {path: '/owned/Fixture.app'}, executableURL: {path: '/owned/Fixture.app/Contents/MacOS/native_world_fixture'}};
    }, runningApplicationsWithBundleIdentifier(id) {
      calls.push('target'); fail('target'); targetReads++;
      assert.equal(id, 'com.apple.systemevents');
      return {count: options.targetAbsent ? 0 : options.ambiguousTarget ? 2 : 1, objectAtIndex(index) {
        assert.equal(index, 0);
        return {processIdentifier: options.replacedTarget && targetReads > 1 ? 45 : 44, isTerminated: false,
          bundleIdentifier: options.foreignTarget ? 'foreign.app' : id,
          bundleURL: {path: '/System/Library/CoreServices/System Events.app'},
          executableURL: {path: options.wrongExecutable ? '/foreign' : '/System/Library/CoreServices/System Events.app/Contents/MacOS/System Events'}};
      }};
    }},
    NSThread: {sleepForTimeInterval(seconds) { assert.equal(seconds, 0.05); sleeps++; }},
    NSAppleEventDescriptor: {descriptorWithBundleIdentifier(id) {
      calls.push('factory'); assert.equal(id, 'com.apple.systemevents'); fail('factory'); return target;
    }},
    NSDictionary: dictionaryClass,
    CGSessionCopyCurrentDictionary() {
      calls.push('session'); fail('session');
      return options.nullSession ? null : sessionDictionary;
    },
    CFRelease(value) { assert.equal(value, sessionDictionary); calls.push('release'); fail('release'); },
    sessionValues() {
      if (Object.hasOwn(options, 'session')) return options.session;
      const session = {kCGSSessionUserIDKey: 41, kCGSSessionOnConsoleKey: true,
        kCGSessionLoginDoneKey: true, CGSSessionScreenIsLocked: options.locked === true};
      return session;
    },
    NSScreen: {screens: {get count() { calls.push('screens'); return options.screens ?? 1; }}},
  };
  const ObjC = {
    import() {}, deepUnwrap(value) {
      assert.equal(value, sessionDictionary); calls.push('unwrap'); fail('unwrap'); return $.sessionValues();
    }, unwrap: value => value,
    bindFunction(name, signature) {
      bindings.push([name, JSON.parse(JSON.stringify(signature))]);
      if (name === 'AEDeterminePermissionToAutomateTarget') {
        calls.push('bind'); fail('bind');
        if (options.missingFunction) return;
        $[name] = (address, eventClass, eventId, askUserIfNeeded) => {
          calls.push('permission'); fail('permission');
          assert.equal(address, pointer);
          assert.equal(eventClass, 0x2a2a2a2a); assert.equal(eventId, 0x2a2a2a2a);
          assert.equal(askUserIfNeeded, false);
          return options.status ?? 0;
        };
      } else if (name === 'CGSessionCopyCurrentDictionary') {
        calls.push('sessionBind'); fail('sessionBind');
      } else {
        assert.equal(name, 'getuid'); $[name] = () => { calls.push('uid'); return 41; };
      }
    },
  };
  const ctx = {module: {exports: {}}, ObjC, $, Application(id) {
    applicationCalls++; assert.equal(id, 'com.apple.systemevents');
    if (!options.surface) throw new Error('NO_APPLICATION_QUERY');
    return options.surface;
  }};
  new vm.Script(source).runInNewContext(ctx);
  const operation = options.operation || 'probe';
  const raw = ctx.module.exports.run(operation === 'probe' ? ['probe', '0', '/', '/']
    : [operation, '42', '/owned/Fixture.app', '/owned/Fixture.app/Contents/MacOS/native_world_fixture']);
  assert.equal(applicationCalls, operation === 'probe' ? 0 : 1, 'only explicit controlled UI doubles reach System Events');
  assert.equal(raw.includes('PRIVATE'), false, 'native exception text is withheld');
  return {record: JSON.parse(raw), calls, bindings, sleeps};
}
const success = probe();
assert.deepEqual(success.bindings, [
  ['AEDeterminePermissionToAutomateTarget', ['int', ['void *', 'unsigned int', 'unsigned int', 'bool']]],
  ['getuid', ['unsigned int', []]],
  ['CGSessionCopyCurrentDictionary', ['id', []]],
]);
assert.deepEqual(success.calls, ['target', 'bind', 'factory', 'descriptor', 'target', 'permission', 'sessionBind', 'session', 'unwrap', 'release', 'screens', 'uid']);
assert.deepEqual(success.record, {version: 1, code: 0, ax: 1, appleEvents: 1, gui: 1,
  probePhase: 6, aeFunction: 1, aeStatusKind: 0, targetRunning: 1, screenPresent: 2, sessionPresent: 2,
  uidMatch: 2, onConsole: 2, loginDone: 2, lockKnown: 2, lockState: 1, matched: 0, acted: 0, mainVisible: 0, windows: 0});
for (const [status, kind] of [[-1744, 1], [-1743, 2], [-600, 3], [-50, 4], [9001, 4]]) {
  const failure = probe({status});
  assert.equal(failure.record.code, 3); assert.equal(failure.record.probePhase, 4);
  assert.equal(failure.record.aeStatusKind, kind);
  assert.equal(failure.record.appleEvents, 0); assert.equal(failure.record.aeFunction, 1);
  assert.equal(failure.calls.includes('session'), false);
  assert.equal(JSON.stringify(failure.record).includes(String(status)), false, 'raw OSStatus never escapes');
}
for (const [stage, phase] of [['bind', 1], ['factory', 2], ['descriptor', 3], ['permission', 4], ['session', 5]]) {
  const {record} = probe({throwAt: stage});
  assert.equal(record.code, 8); assert.equal(record.probePhase, phase);
  assert.equal(record.aeFunction, stage === 'bind' ? 0 : 1);
  assert.equal(record.appleEvents, stage === 'session' ? 1 : 0);
  assert.equal(record.aeStatusKind, stage === 'session' ? 0 : 4);
  assert.equal(record.gui, 0); assert.equal(record.acted, 0);
}
const missing = probe({missingFunction: true});
assert.equal(missing.record.code, 8); assert.equal(missing.record.probePhase, 1);
assert.equal(missing.record.aeFunction, 0); assert.deepEqual(missing.calls, ['target', 'bind']);
const nil = probe({nilPointer: true});
assert.equal(nil.record.code, 8); assert.equal(nil.record.probePhase, 3);
assert.equal(nil.calls.includes('permission'), false);
const unknown = probe({status: {unknown: 1}});
assert.equal(unknown.record.code, 8); assert.equal(unknown.record.probePhase, 4);
assert.equal(unknown.record.appleEvents, 0);
assert.equal(unknown.record.aeStatusKind, 4);
const untrusted = probe({ax: false});
assert.equal(untrusted.record.code, 2); assert.equal(untrusted.record.probePhase, 0);
assert.deepEqual(untrusted.calls, []);
const locked = probe({locked: true});
assert.equal(locked.record.code, 4); assert.equal(locked.record.probePhase, 5);
assert.equal(locked.record.appleEvents, 1); assert.equal(locked.record.gui, 0);
for (const option of ['targetAbsent', 'ambiguousTarget', 'foreignTarget', 'wrongExecutable', 'replacedTarget']) {
  const result = probe({[option]: true});
  assert.equal(result.record.code, 5); assert.equal(result.record.probePhase, 7);
  assert.equal(result.record.targetRunning, 0); assert.equal(result.record.aeStatusKind, 4);
  assert.equal(result.calls.includes('permission'), false);
  assert.equal(result.sleeps, option === 'targetAbsent' ? 19 : 0);
}
const targetError = probe({throwAt: 'target'});
assert.equal(targetError.record.code, 8); assert.equal(targetError.record.probePhase, 7);
assert.equal(targetError.record.targetRunning, 0);
// Each result executes the actual production run(), not a copied parser.
const validSession = {kCGSSessionUserIDKey: 41, kCGSSessionOnConsoleKey: true,
  kCGSessionLoginDoneKey: true, CGSSessionScreenIsLocked: false};
const sessionCases = [
  [{session: null}, {screenPresent: 2, sessionPresent: 1, uidMatch: 0, onConsole: 0, loginDone: 0, lockKnown: 0, lockState: 0}],
  [{screens: 0}, {screenPresent: 1, sessionPresent: 2, uidMatch: 0, onConsole: 2, loginDone: 2, lockKnown: 2, lockState: 1}],
  [{session: {...validSession, kCGSSessionUserIDKey: 42}}, {uidMatch: 1}],
  [{session: {...validSession, kCGSSessionUserIDKey: 'PRIVATE_UID'}}, {uidMatch: 0}],
  [{session: {...validSession, kCGSSessionOnConsoleKey: false}}, {onConsole: 1}],
  [{session: {...validSession, kCGSSessionOnConsoleKey: 1}}, {onConsole: 0}],
  [{session: {...validSession, kCGSessionLoginDoneKey: false}}, {loginDone: 1}],
  [{session: {...validSession, kCGSessionLoginDoneKey: 'PRIVATE_LOGIN'}}, {loginDone: 0}],
  [{session: {...validSession, CGSSessionScreenIsLocked: true}}, {lockKnown: 2, lockState: 2}],
  [{session: {...validSession, CGSSessionScreenIsLocked: 0}}, {lockKnown: 2, lockState: 0}],
  [{session: {}}, {sessionPresent: 2, uidMatch: 0, onConsole: 0, loginDone: 0, lockKnown: 1, lockState: 0}],
];
for (const [options, fields] of sessionCases) {
  const {record, calls} = probe(options);
  assert.equal(record.code, 4); assert.equal(record.probePhase, 5); assert.equal(record.gui, 0);
  assert.equal(record.acted, 0); assert.equal(record.appleEvents, 1);
  for (const [key, expected] of Object.entries(fields)) assert.equal(record[key], expected, key);
  assert.equal(calls.filter(value => value === 'session').length, 1);
  assert.equal(calls.filter(value => value === 'release').length, 1);
  assert.equal(calls.filter(value => value === 'unwrap').length, 1);
  assert.equal(calls.filter(value => value === 'screens').length, 1);
  assert.equal(calls.filter(value => value === 'uid').length, options.session === null || options.screens === 0 ? 0 : 1);
}
// The optional absent lock flag keeps exactly its previous allowance; truthy
// bridged numbers/strings above remain refused. No diagnostic grants a permit.
const absentLock = {...validSession}; delete absentLock.CGSSessionScreenIsLocked;
const absent = probe({session: absentLock});
assert.equal(absent.record.code, 0); assert.equal(absent.record.gui, 1);
assert.equal(absent.record.lockKnown, 1); assert.equal(absent.record.lockState, 0);
// Exhaust the fixed true/false/missing/wrong-type values against the original
// strict gate. The only native values in this VM come from controlled doubles.
for (const onConsole of [true, false, undefined, 1]) {
  for (const loginDone of [true, false, undefined, 1]) {
    for (const lock of [true, false, undefined, 0]) {
      const session = {...validSession, kCGSSessionOnConsoleKey: onConsole, kCGSessionLoginDoneKey: loginDone};
      if (lock === undefined) delete session.CGSSessionScreenIsLocked;
      else session.CGSSessionScreenIsLocked = lock;
      const expected = onConsole === true && loginDone === true && (lock === undefined || lock === false);
      const {record} = probe({session});
      assert.equal(record.gui, expected ? 1 : 0);
      assert.equal(record.code, expected ? 0 : 4);
    }
  }
}
// The actual one-sample production binding owns its Copy result. Every non-nil
// path releases once, including unknown class, conversion and gate failures.
for (const option of ['nullSession', 'nilSession', 'foreignSession']) {
  const {record, calls} = probe({[option]: true});
  assert.equal(record.code, 4); assert.equal(record.probePhase, 5);
  assert.equal(record.gui, 0); assert.equal(record.acted, 0);
  assert.equal(record.sessionPresent, option === 'foreignSession' ? 0 : 1);
  assert.equal(calls.filter(value => value === 'session').length, 1);
  assert.equal(calls.filter(value => value === 'release').length, option === 'foreignSession' ? 1 : 0);
  assert.equal(calls.includes('unwrap'), false); assert.equal(calls.includes('screens'), false);
}
for (const stage of ['sessionBind', 'session', 'class', 'unwrap', 'release']) {
  const {record, calls} = probe({throwAt: stage});
  assert.equal(record.code, 8); assert.equal(record.probePhase, 5);
  assert.equal(record.gui, 0); assert.equal(record.acted, 0);
  assert.equal(calls.filter(value => value === 'session').length, stage === 'sessionBind' ? 0 : 1);
  assert.equal(calls.filter(value => value === 'release').length, ['class', 'unwrap', 'release'].includes(stage) ? 1 : 0);
  assert.equal(calls.includes('screens'), false);
}
// The actual UI path uses batched role metadata and operation-local children.
// These doubles throw if the scan asks for children of terminal text/images.
function surfaceTree(extra = {}) {
  const counts = {children: 0, batch: 0, role: 0, name: 0, windows: 0, clicks: 0};
  function item(role, name, children = [], value = name) {
    const result = {_role: role, children, role() { counts.role++; return role; },
      name() { counts.name++; return typeof name === 'function' ? name() : name; }, value: () => value,
      position: () => [120, 130], size: () => [100, 30], enabled: () => true};
    const collection = () => {
      counts.children++;
      assert.equal(['AXStaticText', 'AXImage'].includes(role), false, 'terminal content is not expanded');
      return result.children;
    };
    collection.role = () => { counts.batch++; return result.children.map(child => child._role); };
    result.uiElements = collection;
    return result;
  }
  const label = item('AXStaticText', 'Fixture Nova');
  const autopilot = item('AXButton', extra.label || 'Autopilot', [item('AXImage', '')]);
  const actor = item('AXButton', '', [item('AXGroup', '', [label])]);
  const content = item(extra.containerRole || 'AXWebArea', '', [autopilot, actor,
    ...Array.from({length: extra.leaves ?? 800}, () => item('AXStaticText', 'PRIVATE_UNUSED_TEXT'))]);
  if (extra.duplicate) content.children.push(item('AXButton', 'Autopilot'));
  const main = item('AXWindow', 'OrbitPal Native World Fixture', [content]);
  main.position = () => [100, 100]; main.size = () => [800, 700];
  const owner = {windows() { counts.windows++; return [main]; },
    menuBars: () => [{}, {menuBarItems: () => [{}]}]};
  let frontmost = false;
  Object.defineProperty(owner, 'frontmost', {get: () => () => frontmost, set: value => { frontmost = value; }});
  const surface = {applicationProcesses: {whose(query) { assert.equal(query.unixId, 42); assert.deepEqual(Object.keys(query), ['unixId']); return () => [owner]; }},
    click(value) { assert.equal(value, extra.operation === 'nova' ? actor : autopilot); counts.clicks++; }};
  return {surface, counts, main, content, actor, autopilot, label, item};
}
const inspectSurface = surfaceTree();
const inspected = probe({operation: 'inspect', surface: inspectSurface.surface});
assert.equal(inspected.record.code, 0); assert.equal(inspected.record.matched, 1);
assert.equal(inspected.record.acted, 0); assert.equal(inspectSurface.counts.clicks, 0);
assert.equal(inspectSurface.counts.windows, 2, 'one diagnostic window read, one fresh button Main');
assert.ok(inspectSurface.counts.children < 10, '800 static leaves do not cause 800 child queries');
assert.ok(inspectSurface.counts.role < 5, 'roles are batched rather than queried for every leaf');
assert.ok(inspectSurface.counts.name < 10, 'unrelated static text is never named');
const actorSurface = surfaceTree({operation: 'nova'});
const actorReply = probe({operation: 'nova', surface: actorSurface.surface});
assert.equal(actorReply.record.code, 0); assert.equal(actorReply.record.acted, 1);
assert.equal(actorSurface.counts.clicks, 1);
assert.ok(actorSurface.counts.children < 15, 'actor subtree is shared during discovery; only selected target is freshly re-read');
const duplicateSurface = surfaceTree({duplicate: true});
assert.equal(probe({operation: 'autopilot', surface: duplicateSurface.surface}).record.code, 5);
assert.equal(duplicateSurface.counts.clicks, 0);
const unknownContainer = surfaceTree({containerRole: 'AXUnknownContainer'});
assert.equal(probe({operation: 'inspect', surface: unknownContainer.surface}).record.code, 0, 'unknown containers remain searched');
const tooLarge = surfaceTree({leaves: 1200});
assert.equal(probe({operation: 'autopilot', surface: tooLarge.surface}).record.code, 5);
assert.equal(tooLarge.counts.clicks, 0, 'capacity never permits a truncated uniqueness scan');
const changedLabel = surfaceTree();
let labelReads = 0;
changedLabel.autopilot.name = () => ++labelReads === 1 ? 'Autopilot' : 'PRIVATE_CHANGED';
assert.equal(probe({operation: 'autopilot', surface: changedLabel.surface}).record.code, 5);
assert.equal(changedLabel.counts.clicks, 0, 'fresh exact selected label is mandatory');
const changedActor = surfaceTree({operation: 'nova'});
let actorReads = 0;
changedActor.label.name = () => ++actorReads === 1 ? 'Fixture Nova' : 'Fixture Blue';
changedActor.label.value = () => 'Fixture Blue';
assert.equal(probe({operation: 'nova', surface: changedActor.surface}).record.code, 5);
assert.equal(changedActor.counts.clicks, 0, 'selected actor text is freshly re-read');
const ambiguousActor = surfaceTree({operation: 'nova'});
ambiguousActor.content.children.push(ambiguousActor.item('AXButton', '', [ambiguousActor.item('AXStaticText', 'Fixture Nova')]));
assert.equal(probe({operation: 'nova', surface: ambiguousActor.surface}).record.code, 5);
assert.equal(ambiguousActor.counts.clicks, 0, 'actor discovery still requires uniqueness');
const mismatched = surfaceTree();
mismatched.content.uiElements.role = () => [];
assert.equal(probe({operation: 'autopilot', surface: mismatched.surface}).record.code, 5);
assert.equal(mismatched.counts.clicks, 0, 'changed collection metadata cannot authorize action');
const unknownRole = surfaceTree();
unknownRole.content.uiElements.role = () => unknownRole.content.children.map(() => null);
assert.equal(probe({operation: 'autopilot', surface: unknownRole.surface}).record.code, 5);
assert.equal(unknownRole.counts.clicks, 0, 'untyped native role metadata refuses');
const sharedBudget = {nodes: 1199};
const budgetTree = surfaceTree({leaves: 0});
assert.throws(() => nativeTree(budgetTree.main, sharedBudget).walk(budgetTree.main));
console.log('native_world_macos_batched_inspection_passed');
console.log('native_world_macos_probe_stub_passed');
