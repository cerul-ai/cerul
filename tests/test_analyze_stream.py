"""Opt-in local streaming/structured analysis checks; no hosted requests."""
import http.server
import json
import os
from pathlib import Path
import re
import select
import shutil
import subprocess
import tempfile
import threading
import time
import unittest


class Server(http.server.ThreadingHTTPServer):
    def __init__(self):
        self.calls=[]
        self.bad=False
        self.gate=None
        super().__init__(("127.0.0.1",0),Handler)


class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self,*_): pass
    def do_POST(self):
        body=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        gemini='contents' in body
        parts=body['contents'][0]['parts'] if gemini else body['messages'][0]['content']
        prompt=parts[0]['text']
        times=json.loads(re.search(r'Actual sample timestamps: (\[[^\]]+\])',prompt)[1])
        answer={'answer':'Found the target 中文.', 'evidence':[{'time_us':times[0], 'description':'A visible frame.'}], 'limitations':['Sparse samples.']}
        raw=json.dumps(answer,ensure_ascii=False)
        self.server.calls.append((self.path,body))
        stream='streamGenerateContent' in self.path or body.get('stream',False)
        if not stream:
            value={'candidates':[{'finishReason':'STOP','content':{'parts':[{'text':raw}]}}]} if gemini else {'choices':[{'finish_reason':'stop','message':{'content':raw}}]}
            data=json.dumps(value).encode();self.send_response(200);self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data);return
        self.send_response(200);self.send_header('Content-Type','Text/Event-Stream; charset=utf-8');self.send_header('Connection','close');self.end_headers()
        try:
            pieces=[raw[:36],raw[36:]]
            for i,piece in enumerate(pieces):
                value={'candidates':[{'content':{'parts':[{'text':piece}]}}]} if gemini else {'choices':[{'delta':{'content':piece},'finish_reason':None}]}
                payload=('data: '+json.dumps(value)+'\r\n\r\n').encode()
                # Split the wire at awkward boundaries to exercise incremental SSE parsing.
                for j in range(0,len(payload),7): self.wfile.write(payload[j:j+7]);self.wfile.flush()
                if i==0 and self.server.gate is not None: self.server.gate.wait(15)
                time.sleep(.02)
            if not self.server.bad:
                value={'candidates':[{'finishReason':'STOP'}],'usageMetadata':{'totalTokenCount':10}} if gemini else {'choices':[{'delta':{},'finish_reason':'stop'}]}
                self.wfile.write(('data: '+json.dumps(value)+'\n\n').encode());self.wfile.flush()
        except (BrokenPipeError,ConnectionResetError): pass
        self.close_connection=True


@unittest.skipUnless(os.environ.get('CERUL_TEST_BINARY'),'requires a built CLI')
class StreamTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory(prefix='cerul-stream-');self.addCleanup(self.temp.cleanup)
        self.root=Path(self.temp.name);self.source=self.root/'video.mp4'
        subprocess.run(['ffmpeg','-v','error','-f','lavfi','-i','color=size=64x64:rate=1:duration=8','-c:v','libx264',str(self.source)],check=True)
        self.ref=self.root/'ref.png';shutil.copyfile(Path(__file__).parent/'fixtures/ocr-text.png',self.ref)
        self.server=Server();threading.Thread(target=self.server.serve_forever,daemon=True).start()
        self.addCleanup(self.server.server_close);self.addCleanup(self.server.shutdown)
        self.env={'HOME':str(self.root),'PATH':os.environ['PATH'],'GEMINI_API_KEY':'local','OPENAI_API_KEY':'local','NO_PROXY':'127.0.0.1'}
    def command(self,*extra,kind='gemini'):
        return [os.environ['CERUL_TEST_BINARY'],'--json','--workspace',str(self.root/'workspace'),'--set',f'vision.base_url="http://127.0.0.1:{self.server.server_port}"','--set',f'vision.kind="{kind}"','--set','vision.model="mock-vision"','analyze',str(self.source),'--prompt','Find the target','--image',str(self.ref),'--from','00:02','--to','00:05',*extra]
    def run_cli(self,*extra,kind='gemini'):
        return subprocess.run(self.command(*extra,kind=kind),env=self.env,capture_output=True,text=True,timeout=90)
    def test_range_references_cache_and_original_summary(self):
        sidecar=Path(str(self.source)+'.cerul');sidecar.mkdir();old=sidecar/'semantic.summary.jsonl';old.write_text('preserved')
        result=self.run_cli();self.assertEqual(result.returncode,0,result.stdout+result.stderr)
        value=json.loads(result.stdout)['streams'][0]['response']
        self.assertEqual(value['range_us'],[2000000,5000000]);self.assertTrue(value['reference_hashes'])
        self.assertTrue(all(2000000<=t<5000000 for t in value['sample_times_us']))
        self.assertEqual(old.read_text(),'preserved')
        self.assertFalse((sidecar/'embeddings').exists())
        count=len(self.server.calls);result=self.run_cli();self.assertEqual(result.returncode,0,result.stdout+result.stderr)
        self.assertTrue(json.loads(result.stdout)['streams'][0]['cached']);self.assertEqual(len(self.server.calls),count)
        shutil.copyfile(Path(__file__).parent/'fixtures/hand-opencv.png',self.ref)
        result=self.run_cli();self.assertEqual(result.returncode,0,result.stdout+result.stderr);self.assertEqual(len(self.server.calls),count+1)
    def test_gemini_delta_arrives_before_final_json(self):
        self.server.gate=threading.Event()
        process=subprocess.Popen(self.command('--stream'),env=self.env,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        received=b'';deadline=time.monotonic()+45
        try:
            while time.monotonic()<deadline and b'"event":"analysis_delta"' not in received:
                if select.select([process.stderr],[],[],.2)[0]: received+=os.read(process.stderr.fileno(),65536)
                if process.poll() is not None: break
            self.assertIn(b'"event":"analysis_delta"',received)
            self.assertIsNone(process.poll());self.assertFalse(select.select([process.stdout],[],[],0)[0])
        finally: self.server.gate.set()
        out,err=process.communicate(timeout=30);self.assertEqual(process.returncode,0,out+err)
        self.assertEqual(json.loads(out)['streams'][0]['response']['answer']['answer'],'Found the target 中文.')
        self.assertIn('alt=sse',self.server.calls[0][0])
    def test_openai_stream_and_incomplete_stream_are_distinct(self):
        result=self.run_cli('--stream',kind='openai');self.assertEqual(result.returncode,0,result.stdout+result.stderr)
        self.assertIn('analysis_delta',result.stderr)
        self.assertTrue(self.server.calls[-1][1]['stream'])
        self.server.bad=True
        result=self.run_cli('--stream','--recompute');self.assertEqual(result.returncode,6,result.stdout+result.stderr)
        self.assertIsNone(json.loads(result.stdout)['streams'][0]['response'])
        self.assertIn('ended before',result.stdout)

    def test_human_stream_and_invalid_range(self):
        command = self.command('--stream')
        command.remove('--json')
        result = subprocess.run(command, env=self.env, capture_output=True, text=True, timeout=90)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(result.stdout.count('Found the target 中文.'), 1)
        self.assertNotIn('"answer":', result.stdout)
        command = self.command()
        command[command.index('--to') + 1] = '00:09'
        count = len(self.server.calls)
        result = subprocess.run(command, env=self.env, capture_output=True, text=True, timeout=90)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(len(self.server.calls), count)

    def test_multiple_human_streams_have_source_boundaries(self):
        other = self.root / 'other.mp4'
        subprocess.run(['ffmpeg', '-v', 'error', '-f', 'lavfi', '-i',
                        'color=red:size=64x64:rate=1:duration=8', '-c:v',
                        'libx264', str(other)], check=True)
        command = self.command('--stream', str(other))
        command.remove('--json')
        result = subprocess.run(command, env=self.env, capture_output=True, text=True, timeout=90)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        answer = 'Found the target 中文.'
        self.assertEqual(result.stdout.count(answer), 2)
        between = result.stdout.split(answer)[1]
        self.assertTrue(between.startswith('\n\n'), result.stdout)
        self.assertIn(' · ', between)
        self.assertEqual(len(self.server.calls), 2)
