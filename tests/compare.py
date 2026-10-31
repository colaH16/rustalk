#!/usr/bin/env python3
"""Run against disposable PostgreSQL/NATS only. Requires TEST_ALLOW_RESET=1.
The Go reference is built separately from the commit in NOTICE.
No production credentials, addresses, messages or nicknames belong in this test.
"""
import contextlib
import http.client
import json
import os
from pathlib import Path
import queue
import re
import select
import signal
import socket
import socketserver
import subprocess
import tempfile
import threading
import time
import urllib.error
import urllib.parse
import urllib.request

import psycopg
from psycopg import sql

ROOT = Path(__file__).resolve().parent.parent
PG = dict(host=os.getenv('TEST_DB_HOST', '127.0.0.1'), port=int(os.getenv('TEST_DB_PORT', '25432')),
          user='rustalk_test', password='disposable-test-only')
NATS_PORT = int(os.getenv('TEST_NATS_PORT', '14222'))
GO = os.environ['GOTALK_BINARY']
RUST = os.getenv('RUSTALK_BINARY', str(ROOT / 'target/debug/rustalk'))
checks = 0
processes = []
logs = []


def check(condition, label):
    global checks
    assert condition, label
    checks += 1


def request(port, path, data=None, method=None, raw=False):
    headers = {}
    if data is not None:
        data = (data if isinstance(data, str) else urllib.parse.urlencode(data)).encode()
        headers['Content-Type'] = 'application/x-www-form-urlencoded'
    r = urllib.request.Request(f'http://127.0.0.1:{port}{path}', data=data, headers=headers, method=method)
    try:
        response = urllib.request.urlopen(r, timeout=12)
    except urllib.error.HTTPError as e:
        response = e
    with response:
        body = response.read()
        return response.status, (body if raw or not body else json.loads(body))


def wait_ready(port, path='/readyz'):
    for _ in range(150):
        try:
            if request(port, path, raw=True)[0] == 200:
                return
        except (OSError, urllib.error.URLError):
            pass
        time.sleep(.1)
    raise AssertionError(f'server readiness timeout on test port {port}')


def reset_db(name):
    assert name.startswith('rustalk_test_')
    with psycopg.connect(**PG, dbname='postgres', autocommit=True) as c:
        c.execute(sql.SQL('DROP DATABASE IF EXISTS {} WITH (FORCE)').format(sql.Identifier(name)))
        c.execute(sql.SQL('CREATE DATABASE {}').format(sql.Identifier(name)))
    with psycopg.connect(**PG, dbname=name) as c:
        c.execute((ROOT / 'schema.sql').read_text())


def start(kind, name, port=8080, **extra):
    env = dict(os.environ, DB_HOST=PG['host'], DB_PORT=str(PG['port']), DB_USER=PG['user'],
               DB_PASSWORD=PG['password'], DB_NAME=name, DB_INIT_SCHEMA='false',
               NATS_URL=f'nats://127.0.0.1:{NATS_PORT}', NATS_SUBJECT='chat.global',
               LISTEN_ADDR=f'127.0.0.1:{port}', STATIC_DIR=str(ROOT / 'frontend'),
               POD_NAME=socket.gethostname(), TZ='UTC', RUST_LOG='rustalk=info,warn',
               SSE_KEEPALIVE_SECONDS='1')
    env.update(extra)
    binary = RUST
    if kind == 'go':
        env['DB_HOST'] = f"{PG['host']} port={PG['port']}"
        binary = GO
    log = tempfile.TemporaryFile(mode='w+b')
    p = subprocess.Popen([binary], env=env, cwd=ROOT, stdout=log, stderr=log)
    processes.append(p); logs.append((kind, log))
    wait_ready(port, '/history' if kind == 'go' else '/readyz')
    return p


def stop(p):
    if p.poll() is None:
        p.send_signal(signal.SIGTERM)
        try: p.wait(timeout=7)
        except subprocess.TimeoutExpired:
            p.kill(); p.wait()
            raise AssertionError('shutdown exceeded 7 seconds')


def canonical(value):
    if isinstance(value, list): return [canonical(v) for v in value]
    if isinstance(value, dict):
        if 'time' in value: check(re.fullmatch(r'\d{2}:\d{2}:\d{2}', value['time']), 'time format')
        return {k: canonical(v) for k, v in value.items() if k not in ('time', 'sender_pod')}
    return value


def scenario(kind, name, port):
    reset_db(name)
    p = start(kind, name, port)
    results = []
    def record(path, data=None, method=None):
        response = request(port, path, data, method)
        results.append((path, canonical(response[1]), response[0]))
        check(response[0] == 200, f'{kind} {path} successful')
        return response[1]
    check(record('/history') is None, 'empty history is null')
    check(record('/login?nick=unknown') == {'nickname':'unknown', 'color_code':''}, 'unknown profile')
    for endpoint in ('/send', '/update'):
        record(endpoint)
        record(endpoint, method='PUT')
        record(endpoint, {})
    record('/send', {'msg':'ignored', 'nick':''})
    record('/send', {'msg':'', 'nick':'ignored'})
    check(record('/history') is None, 'invalid sends do not persist')
    record('/update', {'nick':'한글 🦀'})
    check(record('/login?nick='+urllib.parse.quote('한글 🦀'))['color_code'] == '#ffffff', 'default color')
    record('/send?nick=query&color=%23123456', 'nick=한글+%F0%9F%A6%80&nick=ignored&msg=first%2Bmessage')
    for i in range(35):
        record('/send', {'nick':f'user-{i%3}', 'msg':f'메시지 {i}\n<>&" 🦀', 'color':'#abcdef'})
    history = record('/history')
    check(len(history) == 30 and history[0]['id'] == 36 and history[-1]['id'] == 7, 'descending 30 rows')
    previous = record('/history?before_id=7')
    check(len(previous) == 6 and previous[-1]['id'] == 1, 'exclusive pagination')
    check(record('/history?before_id=1') is None, 'pagination exhausted')
    check(record('/history?before_id=invalid') is None, 'invalid cursor matches Go zero')
    record('/update', {'nick':'user-0','color':'#123456'})
    updated = record('/history')
    check(all(m['sender_color']=='#123456' for m in updated if m['sender_nick']=='user-0'), 'history joins current profile')
    record('/login?nick=user-0')
    stop(p)
    return results


class Stream:
    def __init__(self, port):
        self.events = queue.Queue()
        self.ended = threading.Event()
        self.conn = http.client.HTTPConnection('127.0.0.1', port, timeout=20)
        self.thread = threading.Thread(target=self.read, daemon=True)
        self.thread.start()
    def read(self):
        try:
            self.conn.request('GET', '/stream?nick=test')
            response = self.conn.getresponse()
            assert response.status == 200
            while line := response.readline():
                if line.startswith(b'data:'):
                    self.events.put(json.loads(line[5:].strip()))
        except (OSError, http.client.HTTPException):
            pass
        finally: self.ended.set()
    def close(self):
        if self.conn.sock:
            with contextlib.suppress(OSError): self.conn.sock.shutdown(socket.SHUT_RDWR)
        self.thread.join(timeout=2)
        self.conn.close()


class ProxyHandler(socketserver.BaseRequestHandler):
    def handle(self):
        upstream = socket.create_connection(('127.0.0.1', NATS_PORT))
        pair = (self.request, upstream)
        self.server.connections.update(pair)
        try:
            while not self.server.disabled.is_set():
                ready, _, _ = select.select(pair, [], [], .1)
                for source in ready:
                    data = source.recv(65536)
                    if not data: return
                    (upstream if source is self.request else self.request).sendall(data)
        except OSError:
            pass
        finally:
            self.server.connections.difference_update(pair)
            upstream.close()


class Proxy(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True
    def __init__(self):
        self.disabled = threading.Event()
        self.connections = set()
        super().__init__(('127.0.0.1', 0), ProxyHandler)
        threading.Thread(target=self.serve_forever, daemon=True).start()
    def disconnect(self):
        self.disabled.set()
        for connection in list(self.connections):
            with contextlib.suppress(OSError): connection.shutdown(socket.SHUT_RDWR)
            connection.close()
    def restore(self): self.disabled.clear()


def mixed_and_faults():
    name = 'rustalk_test_mixed'
    reset_db(name)
    go = start('go', name)
    rs1 = start('rust', name, 18081)
    proxy = Proxy()
    rs2 = start('rust', name, 18082, DB_TIMEZONE='Asia/Seoul', NATS_URL=f'nats://127.0.0.1:{proxy.server_address[1]}')
    streams = [Stream(port) for port in (8080, 18081, 18082)]
    time.sleep(.5)
    for i, port in enumerate((8080, 18081, 18082)):
        check(request(port, '/send', {'nick':'mixed', 'msg':f'from-{port}', 'color':'#abcdef'})[0] == 200, 'mixed send')
        events = [s.events.get(timeout=10) for s in streams]
        check(all(e == events[0] for e in events), 'all Go/Rust replicas receive identical NATS JSON')
        check(events[0]['content'] == f'from-{port}', 'live message content')
        if port == 18082:
            check(request(port, '/history')[1][0]['time'] == events[0]['time'], 'non-UTC database timestamp matches live event')
        time.sleep(.2)
        check(all(s.events.empty() for s in streams), 'no local-echo duplicates')
    check(request(8080, '/history')[1] == request(18081, '/history')[1], 'shared DB history identical')
    # Roll back both writes when message insertion is blocked beyond the DB timeout.
    with psycopg.connect(**PG, dbname=name) as lock:
        lock.execute('LOCK TABLE messages IN ACCESS EXCLUSIVE MODE')
        status, _ = request(18081, '/send', {'nick':'must-rollback', 'msg':'blocked'})
        check(status == 503, 'bounded database lock failure')
    with psycopg.connect(**PG, dbname=name) as c:
        check(c.execute("SELECT count(*) FROM users WHERE nickname='must-rollback'").fetchone()[0] == 0, 'transaction rolls profile back')
        before = c.execute('SELECT count(*) FROM messages').fetchone()[0]
    proxy.disconnect()
    for _ in range(50):
        if request(18082, '/dependencies', raw=True)[0] == 503: break
        time.sleep(.1)
    check(request(18082, '/dependencies', raw=True)[0] == 503, 'NATS loss reported separately')
    check(request(18082, '/readyz', raw=True)[0] == 200, 'NATS loss preserves service readiness')
    check(request(18082, '/', raw=True)[0] == 200, 'static page available during NATS loss')
    check(request(18082, '/history')[0] == 200, 'history available during NATS loss')
    check(request(18082, '/login?nick=mixed')[0] == 200, 'profile available during NATS loss')
    check(streams[2].ended.wait(2), 'NATS disconnect closes SSE for history catch-up')
    check(request(18082, '/livez', raw=True)[0] == 200, 'dependency outage does not fail liveness')
    check(request(18082, '/send', {'nick':'offline','msg':'saved-during-outage'})[0] == 200, 'disconnected send persists to durable history')
    with psycopg.connect(**PG, dbname=name) as c:
        check(c.execute('SELECT count(*) FROM messages').fetchone()[0] == before + 1, 'offline send persisted exactly once')
    proxy.restore(); wait_ready(18082, '/dependencies')
    check(request(18082, '/readyz', raw=True)[0] == 200, 'NATS reconnect recovers readiness')
    recovered = Stream(18082);time.sleep(.2)
    request(18081, '/send', {'nick':'reconnect','msg':'subscription-restored'})
    check(recovered.events.get(timeout=10)['content'] == 'subscription-restored', 'NATS subscription restored')
    started = time.monotonic(); stop(rs2)
    check(time.monotonic()-started < 7 and rs2.returncode == 0, 'SIGTERM exits cleanly with active SSE')
    check(recovered.ended.wait(2), 'SIGTERM closes SSE')
    for s in streams: s.close()
    recovered.close(); proxy.disconnect(); proxy.shutdown(); proxy.server_close()
    stop(rs1); stop(go)


def main():
    assert os.getenv('TEST_ALLOW_RESET') == '1', 'explicit TEST_ALLOW_RESET=1 required for disposable test DB reset'
    expected = scenario('go', 'rustalk_test_go', 8080)
    actual = scenario('rust', 'rustalk_test_rust', 18080)
    check(actual == expected, 'normalized Go/Rust API contract')
    print(f'PASS: {len(expected)} Go/Rust response pairs')
    mixed_and_faults()
    check(request_invalid_config(), 'invalid config fails without exposing secrets')
    print(f'PASS: {checks} assertions including mixed-version SSE, DB rollback, NATS outage/reconnect, graceful shutdown')


def request_invalid_config():
    env = dict(os.environ, LISTEN_ADDR='invalid', DB_PASSWORD='must-not-appear')
    p = subprocess.run([RUST], env=env, capture_output=True, timeout=5)
    return p.returncode != 0 and b'must-not-appear' not in p.stdout+p.stderr


if __name__ == '__main__':
    try:
        main()
    except Exception:
        for kind, log in logs:
            log.seek(0)
            # Test logs contain synthetic messages only.
            print(kind, log.read().decode(errors='replace')[-5000:])
        raise
    finally:
        for p in processes:
            if p.poll() is None:
                p.terminate()
                try: p.wait(timeout=7)
                except subprocess.TimeoutExpired: p.kill(); p.wait()
