const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const html = fs.readFileSync('frontend/index.html', 'utf8');
const script = html.match(/<script>([\s\S]*?)<\/script>/)[1];
function app(fetch) {
  const context = vm.createContext({fetch, console: {error: () => {}}, localStorage: {getItem: () => null}, Map, Math});
  vm.runInContext(script, context);
  const a = context.appData();
  a.$nextTick = () => {};
  return a;
}
test('reconnect pages through more than 30 missing messages and deduplicates live arrivals', async () => {
  const calls = [];
  const a = app(async url => {
    calls.push(url);
    if (calls.length === 1) a.messages.push({id: 65, content: 'live'});
    const upper = calls.length === 1 ? 70 : calls.length === 2 ? 40 : 10;
    return {ok: true, json: async () => Array.from({length: Math.min(upper, 30)}, (_, i) => ({id: upper-i}))};
  });
  a.messages = [{id: 1}];
  await a.catchUp();
  assert.deepEqual(calls, ['/history', '/history?before_id=41', '/history?before_id=11']);
  assert.equal(a.messages.length, 70);
  assert.equal(a.messages[64].content, 'live');
  assert.equal(a.catchingUp, false);
});
test('failed history fetch preserves existing messages and permits retry', async () => {
  const a = app(async () => ({ok: false}));
  a.messages = [{id: 5}];
  await a.catchUp();
  assert.equal(a.messages.length, 1);
  assert.equal(a.catchingUp, false);
});
