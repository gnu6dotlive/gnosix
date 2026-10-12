#!/usr/bin/env python3
"""Isolated native Wayland DnD two-process experiment: same-head / cross-head."""
import os,sys,time,json,subprocess,pathlib,hashlib
R=pathlib.Path(os.environ.get('G07B02I_RUN_ROOT','/mnt/workbench/capsule-donor-01/g07b02i-private'))
P=os.environ.get('G07B02I_PARENT_SIG','efb50993780079460b0cbed1363e2166a2de1d9f_1791647968_1117103218')
N=os.environ.get('G07B02I_NESTED_SIG','efb50993780079460b0cbed1363e2166a2de1d9f_1791808512_1359868706')
COMPOSITOR_PID=int(os.environ.get('G07B02I_NESTED_PID','3625179'))
mode=sys.argv[1]
assert mode in ('same','cross','same2','reverse')
def cmd(args,env=None,check=True):
 return subprocess.run(args,env=env,check=check,capture_output=True,text=True,timeout=5)
def hypr(sig,*args):
 return cmd(['hyprctl',*args],{**os.environ,'HYPRLAND_INSTANCE_SIGNATURE':sig}).stdout.strip()
def cur():return tuple(map(float,hypr(P,'cursorpos').split(',')))
orig=cur();ydsock=R/f'dnd-{mode}.sock'
log=(R/f'dnd-{mode}-daemon.log').open('w')
yd=subprocess.Popen(['ydotoold','-p',str(ydsock),'-P','0600'],stdout=log,stderr=subprocess.STDOUT)
time.sleep(.2)
envyd={**os.environ,'YDOTOOL_SOCKET':str(ydsock)}
envchild={**os.environ,'WAYLAND_DISPLAY':os.environ.get('G07B02I_SOCKET','wayland-2'),'XDG_RUNTIME_DIR':'/run/user/1000'}
envchild.pop('HYPRLAND_INSTANCE_SIGNATURE',None)
procs={}
report={'mode':mode,'initial_cursor':orig}
def move(goal,step_max=32):
 for j in range(180):
  current=cur();delta=(goal[0]-current[0],goal[1]-current[1])
  if max(abs(delta[0]),abs(delta[1]))<=2:return cur()
  step=lambda x:max(-step_max,min(step_max,round(x*.58))) if abs(x)>1 else 0
  cmd(['ydotool','mousemove','-x',str(step(delta[0])),'-y',str(step(delta[1]))],envyd)
  time.sleep(.015)
 raise RuntimeError(('cursor_not_at_target',goal,cur()))
def inner_clients():
 return [x for x in json.loads(hypr(N,'clients','-j')) if x.get('title','').startswith('G07B02I DnD')]
def arrange(title,position):
 rows=[x for x in inner_clients() if x['title']==title]
 assert len(rows)==1,(title,rows)
 address=rows[0]['address']
 for v in (f'hl.dsp.window.float({{action="enable",window="address:{address}"}})',
           f'hl.dsp.window.resize({{x=320,y=240,window="address:{address}"}})',
           f'hl.dsp.window.move({{x={position[0]},y={position[1]},window="address:{address}"}})'):
  result=hypr(N,'dispatch',v)
  assert result=='ok',(v,result)
 time.sleep(.15)
 x=next(y for y in inner_clients() if y['address']==address)
 assert tuple(x['at'])==tuple(position) and x['size']==[320,240],(title,x)
 return x
def map_point(inner):
 monitors=json.loads(hypr(N,'monitors','-j'))
 output=next(m for m in monitors if m['x']<=inner['at'][0]+160<m['x']+m['width'])
 outer=[x for x in json.loads(hypr(P,'clients','-j')) if x.get('pid')==COMPOSITOR_PID]
 assert len(outer)==2 and all(x['size'][0] in (1250,1251) and x['size'][1]==800 for x in outer)
 host=sorted(outer,key=lambda x:x['at'][0])[0 if output['name']=='WAYLAND-1' else 1]
 return (round(host['at'][0]+inner['at'][0]+160-output['x']),
         round(host['at'][1]+inner['at'][1]+120-output['y']))
try:
 for role in ('target','source'):
  fh=(R/f'dnd-{mode}-{role}.log').open('w')
  proc=subprocess.Popen([str(R/'dnd-native'),role],env=envchild,stdout=fh,stderr=subprocess.STDOUT)
  procs[role]=(proc,fh)
 for j in range(65):
  if len(inner_clients())==2 and all('DND_MAPPED' in (R/f'dnd-{mode}-{r}.log').read_text(errors='replace') for r in ('source','target')):break
  time.sleep(.07)
 else:raise RuntimeError(('windows_not_mapped',inner_clients()))
 source=arrange('G07B02I DnD Source',(1410,260) if mode in ('same2','reverse') else (160,260))
 target=arrange('G07B02I DnD Target',(1450,260) if mode=='cross' else (1935,260) if mode=='same2' else (685,260))
 report['source_global']=source['at'];report['target_global']=target['at']
 first=map_point(source);last=map_point(target)
 report['source_cursor']=first;report['target_cursor']=last
 print('DND_POS',mode,first,last,flush=True)
 move(first)
 cmd(['ydotool','click','0x40'],envyd);time.sleep(.20)
 source_log=(R/f'dnd-{mode}-source.log')
 assert 'DND_SOURCE_DRAG_STARTED' in source_log.read_text(errors='replace'),source_log.read_text()
 print('DND_DRAG_STARTED',mode,flush=True)
 # Hold button, move pointer across window boundaries, then release on target.
 move(last,step_max=29)
 time.sleep(.30)
 cmd(['ydotool','click','0x80'],envyd)
 for j in range(70):
  src=(R/f'dnd-{mode}-source.log').read_text(errors='replace')
  dst=(R/f'dnd-{mode}-target.log').read_text(errors='replace')
  if 'DND_TARGET_RECEIVED' in dst and 'DND_SOURCE_FINISHED' in src:break
  if any(p.poll() is not None for p,f in procs.values()):break
  time.sleep(.06)
 report['source_log']=src;report['target_log']=dst
 report['received']='DND_TARGET_RECEIVED bytes=37 valid=1' in dst
 report['source_finished']='DND_SOURCE_FINISHED' in src
 print('DND_OUTCOME',mode,report['received'],report['source_finished'],flush=True)
finally:
 for p,f in procs.values():
  if p.poll() is None:p.terminate()
  try:p.wait(timeout=1)
  except subprocess.TimeoutExpired:p.kill();p.wait(timeout=1)
  f.close()
 try:move(orig)
 except Exception as e:report['restore_exception']=str(e)
 report['cursor_restored']=cur()
 yd.terminate()
 try:yd.wait(timeout=2)
 except subprocess.TimeoutExpired:yd.kill()
 log.close()
 if ydsock.exists():ydsock.unlink()
 (R/f'dnd-{mode}-receipt.json').write_text(json.dumps(report,indent=2)+'\n')
 print('CURSOR_RESTORED',report['cursor_restored'],flush=True)
