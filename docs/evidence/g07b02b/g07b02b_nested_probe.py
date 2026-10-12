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
env={**os.environ,'WAYLAND_DISPLAY':'wayland-2','XDG_RUNTIME_DIR':'/run/user/1000',
 'HYPRLAND_INSTANCE_SIGNATURE':NESTED,'G07B02_NESTED':'1','GNOS_ENVELOPE_G07_INPUT':'1',
 'GNOS_ENVELOPE_G07_OUTPUT':'WAYLAND-1','GNOS_ENVELOPE_G07B_DYNAMIC':'1'}
def hypr(sig,*args):
 return subprocess.check_output(['hyprctl',*args],env={**env,'HYPRLAND_INSTANCE_SIGNATURE':sig},timeout=6,text=True).strip()
def cursor():
 return tuple(map(int,hypr(PARENT,'cursorpos').split(',')))
def outer():
 a=json.loads(hypr(PARENT,'clients','-j'))
 x=[c for c in a if c.get('pid')==NESTED_PID]
 assert len(x)==1 and x[0]['mapped'] and x[0]['size'][0]>=900 and x[0]['size'][1]>=560,x
 return x[0]
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
 p=subprocess.Popen([str(B),'--dock-visible-fraction=1.0'],env={**env,**variables,'WAYLAND_DEBUG':'client'},stdout=out,stderr=err,start_new_session=True)
 opened.append(p)
 for i in range(65):
  time.sleep(.1)
  if p.poll() is not None:raise RuntimeError((filename,'exited',p.returncode))
  found=layers(options,p.pid)
  if len(found)==1 and found[0][0]=='WAYLAND-1' and found[0][1]['w']==900 and found[0][1]['h']==560:
   return p,found[0][1]
 raise RuntimeError(('layer_not_found',filename,found))
try:
 for _ in range(20):
  if sock.exists():break
  time.sleep(.08)
 assert sock.is_socket()
 parent=outer()
 assert tuple(parent['size'])==tuple(json.loads(hypr(NESTED,'monitors','-j'))[0][k] for k in ('width','height'))
 witness,wa=start('witness','gnos.ux:g07:input-witness',
  {'GNOS_ENVELOPE_G07_WITNESS':'1','GNOS_ENVELOPE_G07_WITNESS_ONLY':'1'})
 target=(parent['at'][0]+wa['x']+450,parent['at'][1]+wa['y']+460)
 reached,moves=move(target)
 time.sleep(.2)
 overlay,ov=start('overlay','gnos.ux:dock:atlas-qualification',{})
 assert ov['x']==wa['x'] and ov['y']==wa['y'],('layer_geometry_mismatch',wa,ov)
 t0=time.monotonic()
 while time.monotonic()-t0<3:
  region=latest(O/'overlay.stdout.log')
  if region and contains(region,450,460):break
  time.sleep(.03)
 else:raise RuntimeError(('region_unavailable',region))
 time.sleep(1.3)
 before=latest(O/'overlay.stdout.log')
 assert before and contains(before,450,460),before
 assert cursor()==reached,('cursor_moved_unexpectedly',cursor(),reached)
 prior=clicks(O/'overlay.stdout.log',O/'witness.stdout.log')
 run('ydotool','click','-D','55','0xC0',env_=yen)
 time.sleep(.19)
 post=clicks(O/'overlay.stdout.log',O/'witness.stdout.log')
 delta={k:post[k]-prior[k] for k in prior}
 record={'test':'stationary_initial_overlay','label':LABEL,'parent':parent,
   'nested_instance':NESTED,'nested_pid':NESTED_PID,'witness_layer':wa,'overlay_layer':ov,
   'cursor_prior':initial,'cursor_test':reached,'target':target,'moves_before_overlay':moves,
   'region_before':before,'region_after':latest(O/'overlay.stdout.log'),
   'click_delta':delta,'expected':'overlay','success':delta=={'overlay':1,'witness':0},
   'fixture_sha256':hashlib.sha256(B.read_bytes()).hexdigest()}
 (O/'observation.json').write_text(json.dumps(record,indent=2)+'\n')
 print('G07B02B_RESULT',LABEL,json.dumps({'delta':delta,'expected':'overlay','success':record['success'],'cursor':reached}),flush=True)
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
