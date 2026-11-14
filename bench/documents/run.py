#!/usr/bin/env python3
"""Run actual upstream NativeReActAgent and PR3 Host with shared Rust tools/engine.
No downloads during evaluation; upstream checkout and Python deps must be prepared.
"""
import argparse, hashlib, json, os, platform, re, selectors, statistics, subprocess, sys, tempfile, time
from pathlib import Path
ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
UPSTREAM = '76836f4eba5e0d25cf53b56c2f475a2c2583c1c9'
def digest(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def score(case, answer, events, error=None):
    tools=[e for e in events if e.get('type','tool')=='tool']
    searched=any(e.get('name')=='search_documents' and e.get('success') for e in tools)
    body='\n'.join(line for line in answer.splitlines() if not line.startswith('Source:'))
    forbidden=any(x.lower() in body.lower() for x in case['forbidden'])
    if case['missing']:
        abstains=bool(re.search(r"couldn.t find|not (?:found|contain|include|provided|available)|no (?:verified|matching|relevant|information|evidence)|missing|absent|doesn.t (?:contain|include)|do not (?:contain|include)",answer,re.I))
        return dict(success=searched and abstains and not forbidden and not error, factual=abstains, sourced=searched, forbidden=forbidden)
    verified=[e['output'] for e in tools if e.get('name')=='answer_documents' and e.get('success')]
    factual=all(x.lower() in answer.lower() for x in case['required'])
    # A successful exact-quotation tool result must support every required value,
    # and the final answer must retain those quotations with their actual source ID.
    sourced=all(any(x.lower() in out.lower() and any(s in answer for s in re.findall(r'document:\d+:\d+:[0-9a-f]{32}',out)) and any(line[2:] in answer for line in out.splitlines() if line.startswith('> ') and x.lower() in line.lower()) for out in verified) for x in case['required'])
    return dict(success=factual and sourced and not forbidden and not error, factual=factual,sourced=sourced,forbidden=forbidden)
class Bridge:
    def __init__(self,binary,log):
        self.p=subprocess.Popen([str(binary)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=log,text=True,bufsize=1)
        self.selector=selectors.DefaultSelector();self.selector.register(self.p.stdout,selectors.EVENT_READ)
    def call(self,**request):
        self.p.stdin.write(json.dumps(request,ensure_ascii=False)+'\n');self.p.stdin.flush()
        if not self.selector.select(180): raise TimeoutError('Bridge deadline exceeded')
        line=self.p.stdout.readline()
        if not line: raise RuntimeError('Bridge exited')
        return json.loads(line)
    def close(self):
        self.p.stdin.close()
        try:self.p.wait(timeout=30)
        except subprocess.TimeoutExpired:self.p.kill();self.p.wait()
        self.selector.close()
def upstream_run(bridge,question):
    from openjarvis.agents.native_react import NativeReActAgent
    from openjarvis.engine._stubs import InferenceEngine
    from openjarvis.tools._stubs import BaseTool,ToolSpec
    from openjarvis.core.types import ToolResult
    events=[]; generations=[]
    class Engine(InferenceEngine):
        engine_id='pr3-matched-llamacpp'
        def generate(self,messages,*,model,temperature=.7,max_tokens=512,**kwargs):
            if temperature!=.7 or max_tokens!=512:raise ValueError('Unmatched sampling request')
            out=bridge.call(op='generate',messages=[dict(role=m.role.value,content=m.content) for m in messages]);generations.append(out)
            if out.get('error'):raise RuntimeError(out['error'])
            return out
        async def stream(self,*args,**kwargs):raise NotImplementedError;yield ''
        def list_models(self):return ['matched-4b']
        def health(self):return True
    class Tool(BaseTool):
        def __init__(self,spec):
            self.tool_id=spec['name'];self._spec=ToolSpec(name=spec['name'],description=spec['description'],parameters=json.loads(spec['parameters']))
        @property
        def spec(self):return self._spec
        def execute(self,**params):
            out=bridge.call(op='tool',name=self.tool_id,arguments=params)
            if 'error' in out:out=dict(name=self.tool_id,arguments=params,success=False,output=out['error'],sources=[])
            events.append(out);return ToolResult(tool_name=self.tool_id,content=out['output'],success=out['success'])
    agent=NativeReActAgent(Engine(),'matched-4b',tools=[Tool(s) for s in bridge.call(op='specs')],max_turns=5,temperature=.7,max_tokens=512)
    start=time.monotonic();error=None
    try:answer=agent.run(question).content
    except Exception as exc:answer='';error=str(exc)
    return dict(answer=answer,error=error,events=events,generations=generations,seconds=time.monotonic()-start)
def main():
    p=argparse.ArgumentParser();p.add_argument('--upstream',type=Path,required=True);p.add_argument('--model',type=Path,required=True);p.add_argument('--out',type=Path,required=True);p.add_argument('--binary',type=Path,default=ROOT/'target/debug/pr3-bench-documents');a=p.parse_args()
    actual=subprocess.check_output(['git','-C',str(a.upstream),'rev-parse','HEAD'],text=True).strip()
    if actual!=UPSTREAM:raise SystemExit('Upstream commit does not match frozen manifest')
    if subprocess.check_output(['git','-C',str(a.upstream),'status','--porcelain'],text=True).strip():raise SystemExit('Upstream checkout must be clean')
    if digest(HERE/'corpus.json')!=(HERE/'corpus.sha256').read_text().strip():raise SystemExit('Frozen corpus changed')
    a.out.mkdir(parents=True,exist_ok=False);sys.path.insert(0,str(a.upstream/'src'))
    os.environ['OPENJARVIS_HOME']=str(a.out/'upstream-home');os.environ['DO_NOT_TRACK']='1'
    corpus=json.loads((HERE/'corpus.json').read_text())
    (a.out/'corpus.json').write_bytes((HERE/'corpus.json').read_bytes())
    (a.out/'corpus.sha256').write_bytes((HERE/'corpus.sha256').read_bytes())
    files=sorted([p for d in ['crate','app/pr3-app/src-tauri/src','bench/documents'] for p in (ROOT/d).rglob('*') if p.is_file() and p.suffix in {'.rs','.toml','.py','.json'} and not any(s in p.parts for s in ['target','result','__pycache__'])]+[ROOT/'Cargo.lock',ROOT/'Cargo.toml'])
    manifest=dict(started=time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),upstream=UPSTREAM,pr3_head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),dirty_build=True,source_hashes={str(p.relative_to(ROOT)):digest(p) for p in files},corpus_sha256=digest(HERE/'corpus.json'),model=dict(path=str(a.model.resolve()),sha256=digest(a.model)),binary_sha256=digest(a.binary),platform=platform.platform(),python=sys.version,seeds=corpus['seeds'],engine=dict(n_ctx=4096,max_new_tokens=512,temperature=.7,top_p=.8,top_k=20,presence_penalty=1.5,thinking=False,gpu_layers=999),limits=dict(inference_rounds=5,tool_rounds=4,tool_calls=12,arguments_bytes=4096,tool_output_bytes=8192),scope='Shared retrieval and verified-quote tools; compares agent orchestration, not OpenJarvis native RAG, mobile, security suite or overall product quality.')
    manifest['hardware'] = subprocess.check_output(['sysctl','hw.model','hw.memsize','hw.ncpu'],text=True).strip() if sys.platform == 'darwin' else platform.machine()
    manifest['power'] = subprocess.check_output(['pmset','-g','batt'],text=True).strip() if sys.platform == 'darwin' else 'not recorded'
    (a.out/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
    results=[]
    with (a.out/'bridge.log').open('w') as log,(a.out/'results.jsonl').open('w') as out:
        bridge=Bridge(a.binary,log)
        try:
            for seed in corpus['seeds']:
                for i,case in enumerate(corpus['cases']):
                    for side in (['pr3','openjarvis'] if (i+seed)%2 else ['openjarvis','pr3']):
                        ready=bridge.call(op='init',side=side,seed=seed,model=str(a.model.resolve()),documents=case['documents'])
                        if not ready.get('ready'):raise RuntimeError(ready)
                        question=case['question']+' Use search_documents, then answer_documents with exact relevant quotations. Return the verified answer with its source IDs. If evidence is missing, say so; never guess.'
                        result=bridge.call(op='run',question=question) if side=='pr3' else upstream_run(bridge,question)
                        result.update(id=case['id'],side=side,seed=seed)
                        result['score']=score(case,result.get('answer',''),result.get('events',[]),result.get('error'))
                        results.append(result);out.write(json.dumps(result,ensure_ascii=False)+'\n');out.flush()
                        print(f"{len(results)}/72 {side} {case['id']} seed={seed}: {'PASS' if result['score']['success'] else 'FAIL'} ({result.get('seconds',0):.1f}s)",flush=True)
        finally:bridge.close()
    summary={side:dict(runs=len(rows),success=sum(r['score']['success'] for r in rows),factual=sum(r['score']['factual'] for r in rows),sourced=sum(r['score']['sourced'] for r in rows),errors=sum(bool(r.get('error')) for r in rows),median_seconds=statistics.median(r.get('seconds',0) for r in rows)) for side in ['pr3','openjarvis'] if (rows:=[r for r in results if r['side']==side])}
    (a.out/'summary.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps(summary,indent=2))
if __name__=='__main__':main()
