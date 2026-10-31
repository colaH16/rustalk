import os,sys,json,time,random,subprocess,pathlib,re,urllib.request,statistics
import psycopg
if os.environ.get('BENCH_ALLOW_RESET') != '1':
 raise SystemExit('Set BENCH_ALLOW_RESET=1 only for disposable benchmark PostgreSQL; this resets rustalk_test_benchmark tables.')
ROOT=pathlib.Path(os.environ['RUSTALK_SOURCE']);OUT=pathlib.Path(sys.argv[1]);OUT.mkdir(exist_ok=True,parents=True)
BENCH_HOST=os.environ['BENCH_DEPENDENCY_HOST']
PG=dict(host=BENCH_HOST,port=5432,user='rustalk_test',password='disposable-test-only',dbname='rustalk_test_benchmark')
with psycopg.connect(**{**PG,'dbname':'postgres'},autocommit=True) as c:
 if not c.execute("SELECT 1 FROM pg_database WHERE datname=%s",(PG['dbname'],)).fetchone(): c.execute('CREATE DATABASE rustalk_test_benchmark')
with psycopg.connect(**PG) as c:c.execute((ROOT/'schema.sql').read_text())
variants=['go_gc','go_off','rust'];cases=[('login_c1','/login?nick=bench-user',2000,1,''),('history_c1','/history',2000,1,''),('history_c8','/history',4000,8,''),('send_c1','/send',600,1,'nick=bench-user&color=%23abcdef&msg=benchmark-message'),('send_c4','/send',1200,4,'nick=bench-user&color=%23abcdef&msg=benchmark-message')]
def seed():
 with psycopg.connect(**PG) as c:
  c.execute('TRUNCATE messages,users RESTART IDENTITY')
  c.execute("INSERT INTO users VALUES('bench-user','#abcdef')")
  c.execute("INSERT INTO messages(content,sender_pod,sender_nick,created_at) SELECT repeat('x',128),'bench-pod','bench-user','2030-01-01 12:00:00'::timestamp FROM generate_series(1,100)")
def proc_stats(pid):
 text=pathlib.Path(f'/proc/{pid}/stat').read_text().split(') ',1)[1].split();cpu=(int(text[11])+int(text[12]))/os.sysconf('SC_CLK_TCK')
 status=pathlib.Path(f'/proc/{pid}/status').read_text();return cpu,int(re.search(r'VmHWM:\s+(\d+)',status)[1])/1024
def round_value(r): return {k:round(r[k],2) for k in ["p50_us","p99_us","rps","peak_rss_mib","gc_cycles"]}
results=[]
for round_idx in range(3):
 for case,path,n,concurrency,body in cases:
  order=variants[round_idx:]+variants[:round_idx]
  for variant in order:
   seed();label=f'{case}-{variant}-r{round_idx+1}';env=dict(os.environ,DB_HOST=BENCH_HOST,DB_PORT='5432',DB_USER=PG['user'],DB_PASSWORD=PG['password'],DB_NAME=PG['dbname'],DB_INIT_SCHEMA='false',NATS_URL='nats://'+BENCH_HOST+':4222',LISTEN_ADDR='127.0.0.1:8080',STATIC_DIR=str(ROOT/'frontend'),TZ='UTC',GOMAXPROCS='2',TOKIO_WORKER_THREADS='2',GOMEMLIMIT='off',GOGC='off' if variant=='go_off' else '100',GODEBUG='gctrace=1' if variant.startswith('go') else '',RUST_LOG='warn')
   if variant.startswith('go'):env['DB_HOST']=BENCH_HOST
   binary=os.environ['GOTALK_BINARY'] if variant.startswith('go') else str(ROOT/'target/release/rustalk')
   with (OUT/(label+'.log')).open('w+') as log:
    p=subprocess.Popen(['taskset','-c','2,3',binary],env=env,cwd=ROOT,stdout=log,stderr=log)
    try:
     for _ in range(200):
      try:
       with urllib.request.urlopen('http://127.0.0.1:8080/history',timeout=1) as r:r.read()
       break
      except OSError:time.sleep(.05)
     else:raise RuntimeError('not ready')
     cmd=['taskset','-c','4,5',str(pathlib.Path(__file__).parent/'client'),'-url','http://127.0.0.1:8080'+path,'-c',str(concurrency),'-body',body]
     subprocess.check_output(cmd+['-n','100']);time.sleep(.1)
     log.flush();offset=log.tell();before=proc_stats(p.pid)[0]
     measured=json.loads(subprocess.check_output(cmd+['-n',str(n)]));after,rss=proc_stats(p.pid)
     log.flush();log.seek(offset);trace=log.read()
     pauses=[];concurrent=[]
     for match in re.finditer(r'gc \d+ @[^:]+: ([\d.]+)\+([\d.]+)\+([\d.]+) ms clock',trace):
      a,b,c=map(float,match.groups());pauses.extend([a*1000,c*1000]);concurrent.append(b*1000)
     measured.update(case=case,variant=variant,round=round_idx+1,server_cpu_us_per_request=(after-before)*1e6/n,peak_rss_mib=rss,gc_cycles=len(concurrent),gc_stw_total_us=sum(pauses),gc_stw_max_us=max(pauses,default=0),gc_mark_wall_total_us=sum(concurrent))
     results.append(measured);(OUT/'http.json').write_text(json.dumps(results,indent=2)+'\n');print(label,round_value(measured),flush=True)
    finally:
     p.terminate()
     try:p.wait(timeout=10)
     except subprocess.TimeoutExpired:p.kill();p.wait()
   time.sleep(.15)
