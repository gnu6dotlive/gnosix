#!/usr/bin/env python3
"""Independent compositor-button-epoch attribution for G07B-02F.
No PASS based solely on client-rendered geometry; correlate actual compositor
button delivery epoch to client Wayland native click and region/focus transitions.
"""
from pathlib import Path
import json,re,bisect,hashlib
R=Path('/mnt/workbench/capsule-donor-01')
RUNS=('dp2-frame','dp2-timer','hdmi-frame','hdmi-timer')
def parse_lines(path,regex):
    return [m for m in re.finditer(regex,path.read_text(errors='replace'))]
def extract_compositor(path):
    s=path.read_text(errors='replace')
    # coloured logger prefix is permitted; all event markers are anchored by their own token.
    regions=[{'ns':int(m.group(1)),'pid':int(m.group(2)),'old':m.group(3)=='true','new':m.group(4)=='true',
              'focus_before':int(m.group(5))} for m in re.finditer(
      r'G07B02E_REGION epoch_ns=(\d+) pid=(\d+) old=(true|false) new=(true|false) local_x=[-\d.]+ local_y=[-\d.]+ focused=(\d+)',s)]
    focuses=[{'ns':int(m.group(1)),'old':int(m.group(2)),'new':int(m.group(3))} for m in re.finditer(
      r'G07B02E_FOCUS epoch_ns=(\d+) old=(\d+) new=(\d+)',s)]
    buttons=[{'ns':int(m.group(1)),'state':int(m.group(2)),'focus':int(m.group(3))} for m in re.finditer(
      r'G07B02E_BUTTON epoch_ns=(\d+) state=(\d+) pointer_focus=(\d+)',s)]
    return regions,focuses,buttons
COMPS={output:extract_compositor(R/f'g07b02f-{output}-compositor.log') for output in ('dp2','hdmi')}
def client_events(root):
    ret=[]
    for receiver in ('overlay','witness'):
        p=root/f'{receiver}.stdout.log'
        for m in parse_lines(p,r'G07B_(?:OVERLAY|WITNESS)_CLICK epoch_ns=(\d+)'):
            ret.append({'ns':int(m.group(1)),'receiver':receiver})
    return sorted(ret,key=lambda x:x['ns'])
def sample(run):
    output=run.split('-')[0]
    regions,focuses,buttons=COMPS[output]
    root=R/f'g07b02b-{run}'
    original=json.loads((root/'observations.json').read_text())
    events=client_events(root)
    mapping={}
    for reg in regions:
        if not (original['attempts'][0]['inject_epoch_ns'][0]-1_000_000_000
                 <reg['ns']<original['attempts'][-1]['inject_epoch_ns'][1]+1_000_000_000):
            continue
        candidates=[x for x in focuses if 0<=x['ns']-reg['ns']<5_000_000 and x['old']==reg['focus_before']]
        if candidates:
            f=candidates[0]
            old_role='witness' if reg['new'] else 'overlay'
            new_role='overlay' if reg['new'] else 'witness'
            for pointer,role in ((f['old'],old_role),(f['new'],new_role)):
                if pointer:
                    assert pointer not in mapping or mapping[pointer]==role,(run,pointer,mapping[pointer],role)
                    mapping[pointer]=role
    rows=[]
    for item in original['attempts']:
        lo,hi=item['inject_epoch_ns']
        # Bound by injection interval; includes process startup latency only to 10 ms.
        presses=[b for b in buttons if b['state']==1 and lo-4_000_000<=b['ns']<=hi+4_000_000]
        releases=[b for b in buttons if b['state']==0 and lo-4_000_000<=b['ns']<=hi+8_000_000]
        actual=[c for c in events if lo-4_000_000<=c['ns']<=hi+12_000_000]
        unique=len(presses)==len(releases)==len(actual)==1
        current_region_before=None
        if presses:
            past=[r for r in regions if r['ns']<=presses[0]['ns'] and r['pid']]
            current_region_before=past[-1]['new'] if past else None
        rec={'index':item['index'],'inject_ns':[lo,hi],
            'preliminary_wrong':item['correct_if_eligible'] is False,
            'published_membership_before':item['membership_before'],
            'published_membership_after':item['membership_after'],
            'compositor_region_at_press':current_region_before,
            'press':presses,'release':releases,'callback':actual,
            'focus_map':mapping,'unique':unique,
            'correct_compositor_focus':bool(unique and presses[0]['focus'] in mapping and
                              mapping[presses[0]['focus']]==actual[0]['receiver']),
            'correct_applied_region_at_press':bool(unique and current_region_before is not None and
                              presses[0]['focus'] in mapping and
                              mapping[presses[0]['focus']]==('overlay' if current_region_before else 'witness')),
            'region_changed_during_press_release':bool(unique and any(
                              presses[0]['ns']<e['ns']<releases[0]['ns'] for e in regions))}
        rows.append(rec)
    qualified=[x for x in rows if x['unique'] and x['press'][0]['focus'] in mapping]
    assert len(rows)==34,(run,len(rows))
    assert len(qualified)>=24,(run,len(qualified),mapping)
    wrong=[x for x in qualified if not x['correct_compositor_focus']]
    wrong_region=[x for x in qualified if not x['correct_applied_region_at_press']]
    assert not wrong_region,(run,'focus_vs_applied_region_mismatch',[x['index'] for x in wrong_region])
    exceptional=[x for x in rows if x['preliminary_wrong']]
    return {'name':run,'output':output,'policy':original['policy'],
       'frames':original['frames'],'targets':original['retargets'],'matched_complete':original['complete'],
       'client_count':len(events),'mapped_compositor_focus':mapping,
       'injected':len(rows),'qualified':len(qualified),'correct':len(qualified)-len(wrong),
       'wrong':[x['index'] for x in wrong],
       'region_consistent_at_press':sum(x['correct_applied_region_at_press'] for x in qualified),
       'boundary_cross_during_button':sum(x['region_changed_during_press_release'] for x in qualified),
       'preliminary_wrong_examples':exceptional,'rows':rows}
if __name__=='__main__':
    results=[sample(name) for name in RUNS]
    for x in results:
        print('EPOCH_AUDIT',x['name'],'qualified',x['qualified'],'correct',x['correct'],
          'wrong',x['wrong'],'cross_during_button',x['boundary_cross_during_button'],
          'preliminary_wrong',[(y['index'],y['correct_compositor_focus']) for y in x['preliminary_wrong_examples']])
    D=R/'g07b02f-evidence'
    D.mkdir(exist_ok=True)
    o={'schema':'g07b02f-compositor-button-epoch-cross-output-v1','results':results}
    p=D/'button_epoch_audit.json'
    p.write_text(json.dumps(o,indent=2)+'\n')
    print('SHA256',hashlib.sha256(p.read_bytes()).hexdigest())
