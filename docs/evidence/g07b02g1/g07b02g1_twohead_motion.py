#!/usr/bin/env python3
"""G07B02B nested Hyprland stationary click causal witness; no parent config mutation."""
import os,sys,time,json,subprocess,hashlib,re
from pathlib import Path
ROOT=Path('/mnt/workbench/capsule-donor-01')
LABEL=sys.argv[1]
POLICY=sys.argv[4]
assert POLICY in ('frame','timer')
O=ROOT/f'g07b02b-{LABEL}'
assert not O.exists(),O
O.mkdir(mode=0o700)
B=Path('/mnt/sandbox/builds/cargo/debug/gnos-ux-dock-envelope-qualify')
PARENT='efb50993780079460b0cbed1363e2166a2de1d9f_1791647968_1117103218'
NESTED=sys.argv[2]
NESTED_PID=int(sys.argv[3])
OUTPUT=sys.argv[5]
assert OUTPUT in ('WAYLAND-1','WAYLAND-2')
HOST_OUTPUT={'WAYLAND-1':'HDMI-A-1','WAYLAND-2':'DP-2'}[OUTPUT]
env={**os.environ,'WAYLAND_DISPLAY':os.environ.get('G07B02G1_WAYLAND_SOCKET','wayland-2'),'XDG_RUNTIME_DIR':'/run/user/1000',
 'HYPRLAND_INSTANCE_SIGNATURE':NESTED,'G07B02_NESTED':'1','GNOS_ENVELOPE_G07_INPUT':'1',
 'GNOS_ENVELOPE_G07_OUTPUT':OUTPUT,'G07B02_NESTED_OUTPUT':OUTPUT,'GNOS_ENVELOPE_G07B_DYNAMIC':'1'}
def hypr(sig,*args):
 return subprocess.check_output(['hyprctl',*args],env={**env,'HYPRLAND_INSTANCE_SIGNATURE':sig},timeout=6,text=True).strip()
def cursor():
 return tuple(map(int,hypr(PARENT,'cursorpos').split(',')))
def outer():
 a=json.loads(hypr(PARENT,'clients','-j'))
 x=[c for c in a if c.get('pid')==NESTED_PID and c['mapped'] and c['size']==[1250,800]]
 assert len(x)==2,('two_simultaneous_windows_required',x)
 return sorted(x,key=lambda c:c['at'][0])[0 if OUTPUT=='WAYLAND-1' else 1]

def layers(ns,pid):
 a=json.loads(hypr(NESTED,'layers','-j'))
 return [(name,c) for name,body in a.items() for items in body.get('levels',{}).values()
     for c in items if c.get('pid')==pid and c.get('namespace')==ns]
def matches(p,exp):
 if not p.exists():return []
 return list(re.finditer(exp,p.read_text(errors='replace'),re.M))
def latest(p):
 a=matches(p,r'^G07B_REGION epoch_ns=(\d+) visible=([\d.]+) rectangles=(.*)$')
 if not a:return None
 m=a[-1]
 return {'stamp':int(m.group(1)),'fraction':float(m.group(2)),
   'boxes':[list(map(float,b.split(','))) for b in m.group(3).split(';')]}
def contains(region,x,y):
 return any(a<=x<b and c<=y<d for a,c,b,d in region['boxes'])
def clicks(p,w):
 return {'overlay':len(matches(p,r'^G07B_OVERLAY_CLICK ')),
         'witness':len(matches(w,r'^G07B_WITNESS_CLICK '))}
def run(*args,env_=None):
 return subprocess.check_output(args,env=(env_ or env),timeout=5,text=True)
initial=cursor()
sock=O/'ydotool.sock'
ydlog=(O/'ydotoold.log').open('w')
yd=subprocess.Popen(['ydotoold','-p',str(sock),'-P','0600'],stdout=ydlog,stderr=subprocess.STDOUT)
yen={**env,'YDOTOOL_SOCKET':str(sock)}
opened=[];streams=[]
def move(target):
 for i in range(155):
  here=cursor()
  dx=target[0]-here[0];dy=target[1]-here[1]
  if max(abs(dx),abs(dy))<=1:return here,i
  step=lambda v: 0 if v==0 else (1 if v>0 else -1)*min(30,max(1,round(abs(v)*.56)))
  run('ydotool','mousemove','-x',str(step(dx)),'-y',str(step(dy)),env_=yen)
  time.sleep(.04)
 raise RuntimeError(('cursor_unreachable',target,cursor()))
def start(filename,options,variables):
 out=(O/(filename+'.stdout.log')).open('w');err=(O/(filename+'.stderr.log')).open('w')
 streams.extend([out,err])
 p=subprocess.Popen([str(B),'--dock-visible-fraction='+('0.0' if filename=='overlay' else '1.0')],env={**env,**variables,'WAYLAND_DEBUG':'client'},stdout=out,stderr=err,start_new_session=True)
 opened.append(p)
 for i in range(65):
  time.sleep(.1)
  if p.poll() is not None:raise RuntimeError((filename,'exited',p.returncode))
  found=layers(options,p.pid)
  if len(found)==1 and found[0][0]==OUTPUT and found[0][1]['w']==900 and found[0][1]['h']==560:
   return p,found[0][1]
 raise RuntimeError(('layer_not_found',filename,found))
try:
 for _ in range(20):
  if sock.exists():break
  time.sleep(.08)
 assert sock.is_socket()
 parent=outer()
 monitors=json.loads(hypr(NESTED,'monitors','-j'))
 assert len(monitors)==2 and {m['name'] for m in monitors}=={'WAYLAND-1','WAYLAND-2'}
 active=next(m for m in monitors if m['name']==OUTPUT)
 assert active['x']==(0 if OUTPUT=='WAYLAND-1' else 1250)
 center=(parent['at'][0]+625,parent['at'][1]+400)
 physical=json.loads(hypr(PARENT,'monitors','-j'))
 hosts=[m['name'] for m in physical if m['x']<=center[0]<m['x']+m['width'] and m['y']<=center[1]<m['y']+m['height']]
 assert hosts==[HOST_OUTPUT],('host_mismatch',hosts,HOST_OUTPUT)
 print('G07B02G_TWOHEAD',OUTPUT,HOST_OUTPUT,parent['at'],flush=True)
 witness,wa=start('witness','gnos.ux:g07:input-witness',
  {'GNOS_ENVELOPE_G07_WITNESS':'1','GNOS_ENVELOPE_G07_WITNESS_ONLY':'1'})
 target=(parent['at'][0]+wa['x']-active['x']+450,parent['at'][1]+wa['y']+460)
 reached,moves=move(target)
 time.sleep(.2)
 overlay,ov=start('overlay','gnos.ux:dock:atlas-qualification',
    {'GNOS_ENVELOPE_MATCHED':'1','CAPSULE_MATCHED_POLICY':POLICY})
 assert ov['x']==wa['x'] and ov['y']==wa['y'],('layer_geometry_mismatch',wa,ov)
 assert parent['floating'] and tuple(parent['size'])==(1250,800)
 p=O/'overlay.stdout.log'
 w=O/'witness.stdout.log'
 time.sleep(.08)
 first=latest(p)
 start_time=time.monotonic()
 last_click_time=start_time
 attempts=[]
 step=0
 # Real next-frame/timer-driven SharedMotion; NO region-command fixture.
 # Allow 20 retargets at 420 ms, then 850 ms for completion.
 while time.monotonic()-start_time<10.8:
  assert overlay.poll() is None and witness.poll() is None
  now=time.monotonic()
  if now-last_click_time<.135:
   time.sleep(.018);continue
  monitor=outer()
  assert monitor['floating'] and tuple(monitor['size'])==(1250,800)
  assert tuple(monitor['at'])==tuple(parent['at']),('parent_moved',parent,monitor)
  if max(abs(cursor()[i]-reached[i]) for i in (0,1))>1:
   raise RuntimeError(('cursor_moved_without_injection',cursor(),reached))
  pre=latest(p)
  if not pre:
   time.sleep(.028);continue
  step+=1
  counts_before=clicks(p,w)
  press_ns=time.time_ns()
  run('ydotool','click','-D','35','0xC0',env_=yen)
  release_ns=time.time_ns()
  time.sleep(.060)
  post=latest(p)
  counts_after=clicks(p,w)
  changes={k:counts_after[k]-counts_before[k] for k in counts_before}
  # Conservative oracle: membership both before and after click must agree.
  before_class=contains(pre,450,460)
  after_class=contains(post,450,460)
  stable=before_class==after_class
  expected='overlay' if before_class else 'witness'
  expected_counts={'overlay':int(before_class),'witness':int(not before_class)}
  actual_n=sum(changes.values())
  eligible=stable and pre['stamp']<=press_ns and release_ns<=post['stamp']+1_000_000_000
  correct=(changes==expected_counts) if eligible else None
  live=matches(p,r'^MATCHED_FRAME policy=(frame|timer) serial=(\d+) target=(true|false) value=([\d.]+) velocity=([-\d.]+)')
  phases=matches(p,r'^MATCHED_TARGET stage=(\d+) target=(true|false) elapsed_us=(\d+)')
  moving=.04<pre['fraction']<.96 and len(live)>=2
  attempt={'index':step,'elapsed_s':round(now-start_time,4),'phase':int(phases[-1].group(1)) if phases else None,
   'frames_at_click':len(live),'fraction_before':pre['fraction'],'fraction_after':post['fraction'],
   'region_epoch_before':pre['stamp'],'region_epoch_after':post['stamp'],
   'membership_before':before_class,'membership_after':after_class,'membership_stable':stable,
   'classification_eligible':eligible,'animation_in_flight':moving,
   'inject_epoch_ns':[press_ns,release_ns],
   'expected_if_stable':expected,'count_delta':changes,
   'total_click_receivers':actual_n,'correct_if_eligible':correct}
  attempts.append(attempt)
  if step%9==0:
   print('G07B02D_SAMPLE',LABEL,POLICY,step,'eligible',eligible,'inflight',moving,
        'delta',changes,'correct',correct,flush=True)
  last_click_time=time.monotonic()
  if len(matches(p,r'^MATCHED_COMPLETE ')) and time.monotonic()-start_time>=9.5:
   break
  time.sleep(.012)
 frames=matches(p,r'^MATCHED_FRAME policy=(frame|timer) serial=(\d+) target=(true|false) value=([\d.]+) velocity=([-\d.]+)')
 targets=matches(p,r'^MATCHED_TARGET stage=(\d+) target=(true|false) elapsed_us=(\d+)')
 completes=matches(p,r'^MATCHED_COMPLETE policy=(frame|timer) targets=(\d+) callbacks=(\d+) elapsed_us=(\d+)')
 eligible=[q for q in attempts if q['classification_eligible']]
 moving=[q for q in eligible if q['animation_in_flight']]
 results={'schema':'g07b02d-active-motion-v1','label':LABEL,'policy':POLICY,
  'parent_window_geometry':parent,'nested_pid':NESTED_PID,'nested_instance':NESTED,
  'witness_layer':wa,'overlay_layer':ov,'cursor_original':initial,
  'cursor_test':reached,'cursor_moves_before_overlay':moves,
  'fixture_sha256':hashlib.sha256(B.read_bytes()).hexdigest(),
  'frames':len(frames),'retargets':len(targets),'complete':len(completes)>0,
  'complete_details':completes[-1].group(0) if completes else None,
  'last_target':int(targets[-1].group(1)) if targets else None,
  'clicks_injected':len(attempts),'eligible':len(eligible),
  'eligible_correct':sum(q['correct_if_eligible'] is True for q in eligible),
  'eligible_wrong':sum(q['correct_if_eligible'] is False for q in eligible),
  'eligible_in_motion':len(moving),
  'eligible_in_motion_correct':sum(q['correct_if_eligible'] is True for q in moving),
  'eligible_in_motion_wrong':sum(q['correct_if_eligible'] is False for q in moving),
  'not_classifiable':len(attempts)-len(eligible),
  'attempts':attempts}
 (O/'observations.json').write_text(json.dumps(results,indent=2)+'\n')
 print('G07B02D_COMPLETE',LABEL,'policy',POLICY,'frames',len(frames),
  'targets',len(targets),'complete',results['complete'],'classified',
  len(eligible),'correct',results['eligible_correct'],'wrong',results['eligible_wrong'],
  'in_motion',len(moving),'motion_wrong',results['eligible_in_motion_wrong'],flush=True)

finally:
 for p in reversed(opened):
  p.terminate()
  try:p.wait(timeout=3)
  except subprocess.TimeoutExpired:p.kill();p.wait(timeout=2)
 for h in streams:h.close()
 try:
  restored,n=move(initial)
  (O/'cursor-restored.json').write_text(json.dumps({'original':initial,'restored':restored,'moves':n})+'\n')
  print('RESTORED',restored,flush=True)
 finally:
  yd.terminate()
  try:yd.wait(timeout=3)
  except subprocess.TimeoutExpired:yd.kill();yd.wait(timeout=2)
  ydlog.close()
  if sock.exists():sock.unlink()
