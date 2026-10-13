import json,os,subprocess,tempfile,unittest
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
HERE=Path(__file__).parent
class ProcessTest(unittest.TestCase):
 def run_cli(self,inputs):
  with tempfile.TemporaryDirectory() as d:
   mark=Path(d)/'session'
   result=subprocess.run([str(ROOT/'play-cli'),'--worker',str(HERE/'replay_worker.py')],input=inputs.encode(),capture_output=True,env={**os.environ,'CLI_TEST_SESSION_PATH':str(mark)},timeout=20)
   self.assertEqual(result.returncode,0,result.stderr.decode())
   self.assertEqual(result.stderr,b'')
   self.assertFalse(Path(mark.read_text()).parent.exists(),'temporary game must be removed')
   return result.stdout
 def test_quit_and_eof_remove_game(self):
  for entry in ('q\n',''):
   with self.subTest(entry=entry):self.run_cli(entry)
