#!/usr/bin/env python3
"""Independent audit of G07B02D temporal click attribution and region boundaries."""
import json,re,bisect,hashlib
from pathlib import Path
R=Path('/mnt/workbench/capsule-donor-01')
NAMES=('patched-frame','patched-timer','stock-frame','stock-timer')
def parse_regions(path):
 s=path.read_text()
 out=[]
 for m in re.finditer(r'^G07B_REGION epoch_ns=(\d+) visible=([0-9.]+) rectangles=(.*)$',s,re.M):
  rect=[]
  for b in m.group(3).split(';'):
   x1,y1,x2,y2=map(float,b.split(','));rect.append((x1,y1,x2,y2))
  owned=any(x1<=450<x2 and y1<=460<y2 for x1,y1,x2,y2 in rect)
  out.append((int(m.group(1)),owned,float(m.group(2))))
 return out
def click_events(root):
 ev=[]
 for kind in ('overlay','witness'):
  p=root/f'{kind}.stdout.log'
  for m in re.finditer(r'^G07B_(?:OVERLAY|WITNESS)_CLICK epoch_ns=(\d+) (.*)$',p.read_text(),re.M):
   ev.append((int(m.group(1)),kind,m.group(2)))
 return sorted(ev)
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
for name in NAMES:
 root=R/('g07b02b-'+name)
 obs=json.loads((root/'observations.json').read_text())
 regions=parse_regions(root/'overlay.stdout.log')
 events=click_events(root)
 t=[r[0] for r in regions]
 audit=[]
 missing=[]
 for q in obs['attempts']:
  lo,hi=q['inject_epoch_ns']
  region_lo_idx=max(0,bisect.bisect_right(t,lo)-1)
  region_hi_idx=min(len(t),bisect.bisect_right(t,hi+100_000_000))
  current=regions[region_lo_idx:region_hi_idx]
  nearby=[e for e in events if lo-10_000_000<=e[0]<=hi+100_000_000]
  nearest_pre=t[region_lo_idx] if regions else 0
  previous_inside=regions[region_lo_idx][1]
  strong=(q['classification_eligible'] and len(current)>=2
          and all(reg[1]==previous_inside for reg in current)
          and len(nearby)==1 and nearby[0][1]==('overlay' if q['count_delta']['overlay']==1 else 'witness')
          and nearest_pre<=lo and lo-nearest_pre<150_000_000)
  verdict=(nearby[0][1] == ('overlay' if previous_inside else 'witness')) if strong else None
  inflight=q['animation_in_flight'] and strong
  audit.append({'index':q['index'],'strong':strong,'correct':verdict,'inflight':inflight,
                'receivers':[(e[0],e[1]) for e in nearby],
                'regions_in_window':len(current),'pre_age_ns':lo-nearest_pre,
                'all_region_memberships':[reg[1] for reg in current],
                'app_inside_false':bool(nearby and nearby[0][1]=='overlay' and 'inside=false' in nearby[0][2])})
 qualified=[x for x in audit if x['strong']]
 report={'label':name,'preliminary':{'eligible':obs['eligible'],'correct':obs['eligible_correct'],'wrong':obs['eligible_wrong']},
  'independent':{'qualified':len(qualified),'correct':sum(x['correct'] is True for x in qualified),
      'wrong':sum(x['correct'] is False for x in qualified),
      'qualified_inflight':sum(x['inflight'] for x in qualified),
      'inflight_wrong':sum(x['inflight'] and x['correct'] is False for x in qualified)},
  'false_inside_overlay_events':sum(x['app_inside_false'] for x in audit),
  'all_events_count':len(events),'all_regions_count':len(regions),
  'log_hashes':{kind:sha(root/f'{kind}.stdout.log') for kind in ('overlay','witness')},
  'audit':audit}
 (root/'audit.json').write_text(json.dumps(report,indent=2)+'\n')
 print('AUDIT',name,report['independent'],'overlay_inside_false',report['false_inside_overlay_events'],
  'events',len(events),'regions',len(regions))
