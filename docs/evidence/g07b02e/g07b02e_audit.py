#!/usr/bin/env python3
"""G07B-02E independent compositor/client Wayland epoch audit."""
from pathlib import Path
import json,re,hashlib,collections
R=Path('/mnt/workbench/capsule-donor-01')
G=R/'g07b-dynamic-envelope/docs/evidence/g07b02e'
raw=Path('/run/user/1000/hypr/efb50993780079460b0cbed1363e2166a2de1d9f_1791790319_1981226016/hyprland.log')
s=raw.read_text()
h=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
reg=[]
for m in re.finditer(r'G07B02E_REGION epoch_ns=(\d+) pid=(\d+) old=(true|false) new=(true|false) local_x=([-\d.]+) local_y=([-\d.]+) focused=(\d+)',s):
 reg.append({'epoch_ns':int(m.group(1)),'overlay_pid':int(m.group(2)),'old_membership':m.group(3)=='true',
   'new_membership':m.group(4)=='true','local':[float(m.group(5)),float(m.group(6))],
   'focus_before':int(m.group(7))})
focus=[]
for m in re.finditer(r'G07B02E_FOCUS epoch_ns=(\d+) old=(\d+) new=(\d+) local_x=([-\d.]+) local_y=([-\d.]+)',s):
 focus.append({'epoch_ns':int(m.group(1)),'old':int(m.group(2)),'new':int(m.group(3))})
buttons=[]
for m in re.finditer(r'G07B02E_BUTTON epoch_ns=(\d+) state=(\d+) pointer_focus=(\d+) client_available=(true|false) dnd=(true|false)',s):
 buttons.append({'epoch_ns':int(m.group(1)),'state':int(m.group(2)),'focused':int(m.group(3)),
     'client_available':m.group(4)=='true','dnd':m.group(5)=='true'})
# Keep only the two accepted fixture runs; the compositor-wide log also
# contains an explicitly excluded first drag run with teardown error.
windows=[]
for label in ('instrumented-grabs-v3','instrumented-drag-v2'):
    root=R/('g07b02b-'+label)
    clicks=[int(v) for kind in ('overlay','witness')
        for v in re.findall(r'G07B_(?:OVERLAY|WITNESS)_CLICK epoch_ns=(\d+)',
                 (root/(kind+'.stdout.log')).read_text())]
    assert clicks
    windows.append((min(clicks)-2_500_000_000,max(clicks)+500_000_000))
in_window=lambda t:any(a<=t<=b for a,b in windows)
raw_event_counts={'region':len(reg),'focus':len(focus),'button':len(buttons)}
reg=[e for e in reg if in_window(e['epoch_ns'])]
focus=[e for e in focus if in_window(e['epoch_ns'])]
buttons=[e for e in buttons if in_window(e['epoch_ns'])]
assert len(reg)==5 and len(buttons)==22,(raw_event_counts,len(reg),len(buttons),windows)
linked=[]
for e in reg:
 candidates=[f for f in focus if 0 <= f['epoch_ns']-e['epoch_ns']<3_000_000 and f['old']==e['focus_before']]
 assert len(candidates)>=1,('no_focus',e,candidates)
 f=candidates[0]
 e['subsequent_focus_ns']=f['epoch_ns']
 e['focus_after']=f['new']
 e['transition_lag_us']=(f['epoch_ns']-e['epoch_ns'])/1000
 press=[b for b in buttons if b['epoch_ns']<e['epoch_ns']]
 release=[b for b in buttons if b['epoch_ns']>e['epoch_ns']]
 e['within_held_button']=bool(press and release and press[-1]['state']==1 and release[0]['state']==0)
 linked.append(e)
assert sum(e['within_held_button'] for e in linked)==2,linked
def verify_events(label):
 root=R/('g07b02b-'+label)
 o=json.loads((root/'observations.json').read_text())
 assert len(o['events'])==(5 if label=='instrumented-grabs-v3' else 4)
 verdict=[]
 for case in o['events']:
  name=case['case']
  if 'pressed_overlay_then_shrink'==name:
   actual=case['events_after_release']; expected={'overlay':[1,0],'witness':[]}
  elif 'pressed_witness_then_expand'==name:
   actual=case['events_after_release']; expected={'overlay':[],'witness':[1,0]}
  elif name.startswith('double_click_'):
   role=name.removeprefix('double_click_')
   actual=case['events'];expected={'overlay':[1,0,1,0] if role=='overlay' else [],
        'witness':[1,0,1,0] if role=='witness' else []}
  elif name.startswith('post_release_'):
   role=name.removeprefix('post_release_')
   actual=case['event_delta'];expected={'overlay':[1,0] if role=='overlay' else [],
       'witness':[1,0] if role=='witness' else []}
  elif name.startswith('post_drag_click_'):
   role=name.removeprefix('post_drag_click_')
   actual=case['events']; expected={'overlay':[1,0] if role=='overlay' else [],
       'witness':[1,0] if role=='witness' else []}
  elif name=='overlay_drag_into_hole':
   actual=case['after_release'];expected={'overlay':[1,0],'witness':[]}
  elif name=='witness_drag_into_overlay':
   actual=case['after_release'];expected={'overlay':[],'witness':[1,0]}
  else:raise AssertionError(name)
  result=actual==expected
  verdict.append({'case':name,'expected':expected,'observed':actual,'pass':result})
  assert result,(label,name,actual,expected)
 restored=json.loads((root/'cursor-restored.json').read_text())
 assert max(abs(restored['original'][i]-restored['restored'][i]) for i in (0,1))<=1
 return {'label':label,'cases':verdict,'passed':sum(x['pass'] for x in verdict),
    'cursor_restored_within_1px':True,'raw_files':{
    name:h(root/name) for name in ('observations.json','overlay.stderr.log','witness.stderr.log',
                    'overlay.stdout.log','witness.stdout.log')}}
result={
 'schema':'g07b02e-pointer-grab-epoch-independent-v1',
 'source':'Hyprland v0.56.2 local test branch plus G07B02B focus patch',
 'accepted_windows_ns':windows,'full_log_event_counts':raw_event_counts,
 'compositor_file':'g07b02e-compositor-trace.log',
 'compositor_sha256':h(raw),
 'epoch_traces':{'region_transitions':len(reg),'focus_transitions':len(focus),'button_events':len(buttons),
    'region_focus_correlated':len(linked),'region_change_during_pressed_button':
      sum(x['within_held_button'] for x in linked),'max_region_to_focus_lag_us':
      max(x['transition_lag_us'] for x in linked),'regions':linked},
 'client_tests':[verify_events(k) for k in ('instrumented-grabs-v3','instrumented-drag-v2')],
 'qualified_cases':9,'qualified_pass':9,
 'limitations':['Only one 1250x800 nested Wayland output','One clean run per grab and drag fixture',
   'Focus and client pointer delivery can differ during an implicit grab; raw transport logs determine recipient',
   'No multi-output, GPU failure injection, 60Hz stress or source-level upstream acceptance',
   'Earlier drag v1 had cleanup error and was excluded; earlier grab v1/v2 lacked compositor logging and were excluded',
   'Input-region boundary race inside=false from G07B02D was unclassified, not promoted']}
(G/'audit.json').write_text(json.dumps(result,indent=2)+'\n')
print('EPOCH_AUDIT_PASS',result['qualified_pass'],'of',result['qualified_cases'],
 'correlated',len(linked),'held_transitions',sum(x['within_held_button'] for x in linked),
 'lag_us',round(result['epoch_traces']['max_region_to_focus_lag_us'],2))
print('AUDIT_SHA256',h(G/'audit.json'))
