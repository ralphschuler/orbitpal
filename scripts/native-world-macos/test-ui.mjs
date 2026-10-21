// Execute production predicates and the probe in a Node VM with controlled
// ObjC/Application doubles only. No native UI or permission operation runs.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
const context = {module: {exports: {}}};
const source = readFileSync(new URL('./ui.js', import.meta.url), 'utf8');
new vm.Script(source).runInNewContext(context);
const {matchesButton, one} = context.module.exports;
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
  let applicationCalls = 0, targetReads = 0, sleeps = 0;
  const fail = stage => { if (options.throwAt === stage) throw new Error('PRIVATE_NATIVE_ERROR'); };
  Object.defineProperty(target, 'aeDesc', {get() {
    calls.push('descriptor'); fail('descriptor'); return options.nilPointer ? null : pointer;
  }});
  const $ = {
    AXIsProcessTrusted: () => options.ax !== false,
    NSRunningApplication: {runningApplicationsWithBundleIdentifier(id) {
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
    CGSessionCopyCurrentDictionary() {
      calls.push('session'); fail('session');
      return {kCGSSessionUserIDKey: 41, kCGSSessionOnConsoleKey: true,
        kCGSessionLoginDoneKey: true, CGSSessionScreenIsLocked: options.locked === true};
    },
    NSScreen: {screens: {count: 1}},
  };
  const ObjC = {
    import() {}, deepUnwrap: value => value, unwrap: value => value,
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
      } else {
        assert.equal(name, 'getuid'); $[name] = () => 41;
      }
    },
  };
  const ctx = {module: {exports: {}}, ObjC, $, Application() { applicationCalls++; throw new Error('NO_APPLICATION_QUERY'); }};
  new vm.Script(source).runInNewContext(ctx);
  const raw = ctx.module.exports.run(['probe', '0', '/', '/']);
  assert.equal(applicationCalls, 0, 'probe never reaches System Events application queries');
  assert.equal(raw.includes('PRIVATE'), false, 'native exception text is withheld');
  return {record: JSON.parse(raw), calls, bindings, sleeps};
}
const success = probe();
assert.deepEqual(success.bindings, [
  ['AEDeterminePermissionToAutomateTarget', ['int', ['void *', 'unsigned int', 'unsigned int', 'bool']]],
  ['getuid', ['unsigned int', []]],
]);
assert.deepEqual(success.calls, ['target', 'bind', 'factory', 'descriptor', 'target', 'permission', 'session']);
assert.deepEqual(success.record, {version: 1, code: 0, ax: 1, appleEvents: 1, gui: 1,
  probePhase: 6, aeFunction: 1, aeStatusKind: 0, targetRunning: 1, matched: 0, acted: 0, mainVisible: 0, windows: 0});
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
console.log('native_world_macos_probe_stub_passed');
