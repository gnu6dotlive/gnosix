#!/usr/bin/env python3
"""G07B02E held-button boundary and doubleclick; no parent config mutation."""
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
 p=subprocess.Popen([str(B),'--dock-visible-fraction='+('0.0' if filename=='overlay' else '1.0')],env={**env,**variables,'WAYLAND_DEBUG':'client'},stdout=out,stderr=err,start_new_session=True)
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
 command=O/'control.txt'
 command.write_text('0 0.0\n')
 overlay,ov=start('overlay','gnos.ux:dock:atlas-qualification',
    {'G07B02C_COMMAND_FILE':str(command)})
 assert ov['x']==wa['x'] and ov['y']==wa['y'],('layer_geometry_mismatch',wa,ov)
 assert parent['floating'] and tuple(parent['size'])==(1250,800)
 command.write_text('0 0.0\n')
 p=O/'overlay.stdout.log'
 w=O/'witness.stdout.log'
 def region_at(seq,fraction,member):
  epoch=time.time_ns()
  command.write_text(f'{seq} {fraction:.6f}\n')
  started=time.monotonic()
  while time.monotonic()-started<2.8:
   region=latest(p)
   applied=matches(p,r'^G07B02C_APPLY seq=(\d+) fraction=([\d.]+)')
   if (region and region['stamp']>epoch and abs(region['fraction']-fraction)<1e-4
       and any(int(q.group(1))==seq for q in applied)
       and contains(region,450,460)==member):
    time.sleep(.11)
    result=latest(p)
    if result and contains(result,450,460)==member:return result
   time.sleep(.025)
  raise RuntimeError(('region_application_timeout',seq,fraction,region))
 def button_events():
  result={}
  for kind in ('overlay','witness'):
   text=(O/(kind+'.stderr.log')).read_text(errors='replace')
   result[kind]=[int(x) for x in re.findall(r'wl_pointer#\d+\.button\([^\n]*272, ([01])\)',text)]
  return result
 def delta_events(previous):
  current=button_events()
  return {k:current[k][len(previous[k]):] for k in previous}
 def clickcounts():
  return clicks(p,w)
 report={'schema':'g07b02e-grab-boundary-v1','label':LABEL,'nested_instance':NESTED,
  'nested_pid':NESTED_PID,'parent_window':parent,
  'witness_layer':wa,'overlay_layer':ov,
  'fixture_sha256':hashlib.sha256(B.read_bytes()).hexdigest(),
  'events':[]}
 held=False
 try:
  # Phase 1: overlay has focus when button is pressed; remove input coverage while held.
  first=region_at(1,1.0,True)
  time.sleep(.08)
  pre=button_events(); countpre=clickcounts()
  run('ydotool','click','0x40',env_=yen); held=True
  time.sleep(.10)
  afterpress=delta_events(pre)
  assert cursor()==reached,('cursor_moved',cursor(),reached)
  hidden=region_at(2,0.0,False)
  during=delta_events(pre)
  run('ydotool','click','0x80',env_=yen);held=False
  time.sleep(.14)
  endevents=delta_events(pre)
  countend=clickcounts()
  report['events'].append({'case':'pressed_overlay_then_shrink','expected_press':'overlay',
    'expected_implicit_grab_release':'overlay','initial_region':first,'final_region':hidden,
    'press_events':afterpress,'events_during_hold':during,'events_after_release':endevents,
    'click_count_delta':{k:countend[k]-countpre[k] for k in countpre}})
  print('G07B02E_GRAB',LABEL,'overlay_to_witness',endevents,flush=True)
  # After release focus must be on witness if compositor rechecks.
  pre=button_events()
  run('ydotool','click','-D','42','0xC0',env_=yen)
  time.sleep(.13)
  report['events'].append({'case':'post_release_witness','expected':'witness','event_delta':delta_events(pre)})
  # Phase 2: press witness while hidden, then extend overlay before releasing.
  pre=button_events();countpre=clickcounts()
  run('ydotool','click','0x40',env_=yen);held=True
  time.sleep(.10)
  afterpress=delta_events(pre)
  expanded=region_at(3,1.0,True)
  during=delta_events(pre)
  run('ydotool','click','0x80',env_=yen);held=False
  time.sleep(.14)
  endevents=delta_events(pre);countend=clickcounts()
  report['events'].append({'case':'pressed_witness_then_expand','expected_press':'witness',
    'expected_implicit_grab_release':'witness','final_region':expanded,
    'press_events':afterpress,'events_during_hold':during,'events_after_release':endevents,
    'click_count_delta':{k:countend[k]-countpre[k] for k in countpre}})
  print('G07B02E_GRAB',LABEL,'witness_to_overlay',endevents,flush=True)
  # Repeat native full clicks when both clicks must go to the focused layer.
  for seq,fraction,expected in ((4,1.0,'overlay'),(5,0.0,'witness')):
   region=region_at(seq,fraction,expected=='overlay')
   pre=button_events();countpre=clickcounts()
   for _ in range(2):
    run('ydotool','click','-D','45','0xC0',env_=yen)
    time.sleep(.075)
   endevents=delta_events(pre);countend=clickcounts()
   report['events'].append({'case':'double_click_'+expected,'expected':expected,
    'region':region,'events':endevents,'count_delta':{k:countend[k]-countpre[k] for k in countpre}})
   print('G07B02E_DOUBLE',LABEL,expected,endevents,flush=True)
  (O/'observations.json').write_text(json.dumps(report,indent=2)+'\n')
  print('G07B02E_COMPLETE',LABEL,'cases',len(report['events']),flush=True)
 finally:
  if held:
   try:run('ydotool','click','0x80',env_=yen)
   except Exception:pass

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
