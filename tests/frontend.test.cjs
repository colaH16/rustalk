const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const html = fs.readFileSync('frontend/index.html', 'utf8');
const script = html.match(/<script>([\s\S]*?)<\/script>/)[1];
function app(fetch, timers = {}) {
  const context = vm.createContext({fetch, console: {error: () => {}}, localStorage: {getItem: () => null}, Map, Math, setTimeout: timers.setTimeout || (() => 1), clearTimeout: timers.clearTimeout || (() => {})});
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

test('untrusted color cannot add another CSS declaration', () => {
  const a = app(async () => ({}));
  assert.equal(a.safeColor('#aBcD09'), '#aBcD09');
  assert.equal(a.safeColor('red; background-image:url(https://example.test/x)'), '#ffffff');
  assert.equal(a.safeColor(null), '#ffffff');
  assert.ok(html.includes('backgroundColor: safeColor('));
});
test('reconnect during an in-flight catch-up schedules another pass', async () => {
  let calls = 0, release;
  const gate = new Promise(resolve => { release = resolve; });
  const a = app(async () => {
    calls++;
    if (calls === 1) { await gate; return {ok:true, json:async () => [{id:2}]}; }
    return {ok:true, json:async () => [{id:3},{id:2}]};
  });
  a.messages = [{id:1}];
  const first = a.catchUp();
  await a.catchUp();
  release(); await first;
  assert.equal(calls, 2);
  assert.deepEqual(Array.from(a.messages, m => m.id), [1,2,3]);
  assert.equal(a.catchUpPending, false);
});

test('a failed catch-up retries even while SSE stays open', async () => {
  let retry, attempts = 0;
  const a = app(async () => (++attempts === 1 ? {ok:false} : {ok:true, json:async () => [{id:2}]}), {setTimeout: fn => { retry=fn; return 7; }});
  a.messages=[{id:1}]; await a.catchUp();
  assert.equal(a.catchUpRetryTimer,7); assert.equal(typeof retry,'function');
  retry();
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(attempts,2); assert.equal(a.messages.length,2);
});
