// Execute the production predicate in a Node VM. run() is never called; there
// are no ObjC/System Events objects, native UI queries or permission operations.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
const context = {module: {exports: {}}};
new vm.Script(readFileSync(new URL('./ui.js', import.meta.url), 'utf8')).runInNewContext(context);
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
