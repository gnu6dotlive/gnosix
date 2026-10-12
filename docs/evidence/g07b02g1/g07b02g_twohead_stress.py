#!/usr/bin/env python3
"""G07B02B nested Hyprland stationary click causal witness; no parent config mutation."""
import os,sys,time,json,subprocess,hashlib,re
from pathlib import Path
ROOT=Path('/mnt/workbench/capsule-donor-01')
LABEL=sys.argv[1]
O=ROOT/f'g07b02b-{LABEL}'
assert not O.exists(),O
O.mkdir(mode=0o700)
B=Path('/mnt/sandbox/builds/cargo/debug/gnos-ux-dock-envelope-qualify')
PARENT='efb50993780079460b0cbed1363e2166a2de1d9f_1791647968_1117103218'
NESTED=sys.argv[2]
NESTED_PID=int(sys.argv[3])
OUTPUT=sys.argv[4]
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
 command=O/'control.txt'
 command.write_text('0 0.0\n')
 overlay,ov=start('overlay','gnos.ux:dock:atlas-qualification',
    {'G07B02C_COMMAND_FILE':str(command)})
 assert ov['x']==wa['x'] and ov['y']==wa['y'],('layer_geometry_mismatch',wa,ov)
 pattern=[
 ('center-open',450,460,1.0),('center-close',450,460,0.0),
 ('center-half',450,460,0.55),('center-low',450,460,0.15),
 ('left-open',40,470,1.0),('left-close',40,470,0.0),
 ('bottom-open',450,498,1.0),('bottom-close',450,498,0.0),
 ('hole-open',450,270,1.0),('hole-close',450,270,0.0),
 ('margin-open',20,490,1.0),('margin-close',20,490,0.0),
 ('center-open-again',450,460,1.0),('center-close-again',450,460,0.0),
 ('center-reopen',450,460,1.0),('center-reclose',450,460,0.0)]
 assert len(pattern)==16
 results=[]
 for seq,(name,x,y,fraction) in enumerate(pattern,1):
  parent_now=outer()
  assert parent_now['floating'] and tuple(parent_now['size'])==tuple(parent['size']),('geometry_drift',seq,parent_now)
  goal=(parent['at'][0]+ov['x']-active['x']+x,parent['at'][1]+ov['y']+y)
  reached,moves=move(goal)
  time.sleep(.11)
  assert max(abs(cursor()[k]-goal[k]) for k in (0,1))<=1
  previous=latest(O/'overlay.stdout.log')
  delivered=clicks(O/'overlay.stdout.log',O/'witness.stdout.log')
  stamp=time.time_ns()
  command.write_text(f'{seq} {fraction:.6f}\n')
  start_at=time.monotonic()
  while time.monotonic()-start_at<2.1:
   p=O/'overlay.stdout.log'
   reg=latest(p)
   ack=matches(p,r'^G07B02C_APPLY seq=(\d+) fraction=([\d.]+)')
   if (reg and reg['stamp']>stamp and abs(reg['fraction']-fraction)<0.0001
       and any(int(a.group(1))==seq for a in ack)):break
   time.sleep(.017)
  else:raise RuntimeError(('commit_not_verified',seq,fraction,latest(O/'overlay.stdout.log')))
  time.sleep(.09)
  current=latest(O/'overlay.stdout.log')
  assert current and current['stamp']>=reg['stamp'] and abs(current['fraction']-fraction)<.0001
  expected='overlay' if contains(current,x,y) else 'witness'
  before_position=cursor()
  assert max(abs(before_position[k]-reached[k]) for k in (0,1))<=1,('cursor_changed',name,before_position,reached)
  run('ydotool','click','-D','35','0xC0',env_=yen)
  time.sleep(.10)
  after=clicks(O/'overlay.stdout.log',O/'witness.stdout.log')
  delta={k:after[k]-delivered[k] for k in delivered}
  expected_delta={'overlay':1,'witness':0} if expected=='overlay' else {'overlay':0,'witness':1}
  stable=contains(previous,x,y) if previous else None
  new=contains(current,x,y)
  result={'seq':seq,'stage':name,'point':[x,y],'fraction':fraction,
      'global_cursor':before_position,'cursor_moves_before_commit':moves,
      'old_membership':stable,'new_membership':new,'transition':stable is not None and stable!=new,
      'region':current,'expected':expected,'delta':delta,'pass':delta==expected_delta}
  results.append(result)
  print('G07B02C_STEP',LABEL,seq,name,'transition',result['transition'],
       'expected',expected,'observed',delta,'PASS' if result['pass'] else 'FAIL',flush=True)
 receipt={'schema':'g07b02c-dynamic-region-stress-v1','label':LABEL,'nested_instance':NESTED,
     'nested_pid':NESTED_PID,'parent_window_geometry':parent,'witness_layer':wa,
     'overlay_layer':ov,'fixture_sha256':hashlib.sha256(B.read_bytes()).hexdigest(),
     'cursor_before':initial,'steps':results,
     'passes':sum(int(r['pass']) for r in results),
     'transition_count':sum(int(r['transition']) for r in results),
     'incorrect':[(r['seq'],r['expected'],r['delta']) for r in results if not r['pass']]}
 (O/'observations.json').write_text(json.dumps(receipt,indent=2)+'\n')
 print('G07B02C_COMPLETE',LABEL,receipt['passes'],'of',len(results),'transitions',receipt['transition_count'],flush=True)

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
