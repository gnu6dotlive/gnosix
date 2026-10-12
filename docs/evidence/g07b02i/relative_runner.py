#!/usr/bin/env python3
import os,sys,time,json,subprocess,pathlib
R=pathlib.Path(os.environ.get('G07B02I_RUN_ROOT','/mnt/workbench/capsule-donor-01/g07b02i-private'))
N=os.environ.get('G07B02I_NESTED_SIG','efb50993780079460b0cbed1363e2166a2de1d9f_1791808512_1359868706')
P=os.environ.get('G07B02I_PARENT_SIG','efb50993780079460b0cbed1363e2166a2de1d9f_1791647968_1117103218')
mode=sys.argv[1];assert mode in ('confine','lock')
def hypr(sig,*args):
 return subprocess.check_output(['hyprctl',*args],env={**os.environ,'HYPRLAND_INSTANCE_SIGNATURE':sig},text=True,timeout=4).strip()
def cursor():return tuple(map(float,hypr(P,'cursorpos').split(',')))
original=cursor()
sock=R/f'ydotool-{mode}.sock'
ydlog=(R/f'ydotool-{mode}.log').open('w')
daemon=subprocess.Popen(['ydotoold','-p',str(sock),'-P','0600'],stdout=ydlog,stderr=subprocess.STDOUT)
envyd={**os.environ,'YDOTOOL_SOCKET':str(sock)}
out=R/f'constraint-{mode}-executed.log'
handle=out.open('w')
envclient={**os.environ,'WAYLAND_DISPLAY':os.environ.get('G07B02I_SOCKET','wayland-2'),'XDG_RUNTIME_DIR':'/run/user/1000'}
envclient.pop('HYPRLAND_INSTANCE_SIGNATURE',None)
proc=subprocess.Popen([str(R/'relative-lock-probe'),mode],env=envclient,stdout=handle,stderr=subprocess.STDOUT)
def move(target):
 for i in range(130):
  x,y=cursor();dx,dy=target[0]-x,target[1]-y
  if max(abs(dx),abs(dy))<=1.5:return
  step=lambda v: 0 if abs(v)<1 else max(-32,min(32,round(v*.58)))
  subprocess.run(['ydotool','mousemove','-x',str(step(dx)),'-y',str(step(dy))],env=envyd,stdout=subprocess.DEVNULL,check=True,timeout=3)
  time.sleep(.022)
 raise RuntimeError(('not_reached',target,cursor()))
outcome={'mode':mode,'original_cursor':original}
try:
 window=None
 for j in range(35):
  rows=json.loads(hypr(N,'clients','-j'))
  matches=[c for c in rows if c.get('title')=='G07B02I Relative Lock Probe']
  if len(matches)==1 and matches[0]['mapped']:
   window=matches[0];break
  time.sleep(.08)
 assert window,('window_did_not_map',out.read_text())
 target_output=os.environ.get("G07B02I_TARGET_OUTPUT")
 if target_output:
  assert target_output in ("WAYLAND-1","WAYLAND-2")
  address=window["address"]
  def dispatch(cmd):hypr(N,"dispatch",cmd)
  dispatch(f'hl.dsp.window.float({{action="enable",window="address:{address}"}})')
  dispatch(f'hl.dsp.window.resize({{x=320,y=240,window="address:{address}"}})')
  dispatch(f'hl.dsp.window.move({{x={160 if target_output=="WAYLAND-1" else 1410},y=250,window="address:{address}"}})')
  time.sleep(.17)
  window=next(c for c in json.loads(hypr(N,"clients","-j")) if c["address"]==address)
 mons=json.loads(hypr(N,'monitors','-j'))
 selected=next(m for m in mons if m['x']<=window['at'][0]+window['size'][0]/2<m['x']+m['width'])
 assert not target_output or selected['name']==target_output,(selected,window)
 outers=[x for x in json.loads(hypr(P,'clients','-j')) if x.get('pid')==int(os.environ.get('G07B02I_NESTED_PID','3625179')) and x.get('size', [0,0])[0] in (1250,1251) and x.get('size',[0,0])[1]==800]
 assert len(outers)==2,outers
 host=sorted(outers,key=lambda x:x['at'][0])[0 if selected['name']=='WAYLAND-1' else 1]
 goal=(round(host['at'][0]+window['at'][0]+window['size'][0]/2-selected['x']),
       round(host['at'][1]+window['at'][1]+window['size'][1]/2-selected['y']))
 outcome.update(nested_output=selected['name'],client_window=window['at'],client_size=window['size'],target_cursor=goal)
 print('CONSTRAINT_TARGET',mode,selected['name'],goal,flush=True)
 move(goal)
 for j in range(70):
  if 'G2_CONSTRAINT_ACTIVE' in out.read_text(errors='replace'):break
  if proc.poll() is not None:break
  time.sleep(.07)
 if 'G2_CONSTRAINT_ACTIVE' in out.read_text(errors='replace'):
  outcome['inner_before']=tuple(map(float,hypr(N,'cursorpos').split(',')))
  outcome['outer_before']=cursor()
  for k in range(6):
   subprocess.run(['ydotool','mousemove','-x','95','-y','0'],env=envyd,stdout=subprocess.DEVNULL,check=True,timeout=3)
   time.sleep(.03)
  outcome['inner_after']=tuple(map(float,hypr(N,'cursorpos').split(',')))
  outcome['outer_after']=cursor()
  outcome['client_bounds_global']=[window['at'][0],window['at'][1],window['size'][0],window['size'][1]]
  print('MOTION_TEST',mode,'inner',outcome['inner_before'],outcome['inner_after'],'outer',outcome['outer_before'],outcome['outer_after'],flush=True)
 outcome['stdout']=out.read_text(errors='replace')
 outcome['active']='G2_CONSTRAINT_ACTIVE' in outcome['stdout']
 print('CONSTRAINT_RESULT',mode,outcome['active'],flush=True)
finally:
 if proc.poll() is None:proc.terminate()
 try:proc.wait(timeout=2)
 except subprocess.TimeoutExpired:proc.kill();proc.wait(timeout=2)
 time.sleep(.09)
 try:move(original)
 finally:
  outcome['restored']=cursor()
  daemon.terminate()
  try:daemon.wait(timeout=3)
  except subprocess.TimeoutExpired:daemon.kill()
  ydlog.close();handle.close()
  if sock.exists():sock.unlink()
  (R/f'constraint-{mode}-receipt.json').write_text(json.dumps(outcome,indent=2)+'\n')
  print('RESTORED',mode,outcome['restored'],flush=True)
