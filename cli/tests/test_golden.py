"""The latest approved UI contracts are part of default test discovery."""
import sys,unittest
from pathlib import Path
HERE=Path(__file__).parent/'review'
sys.path.insert(0,str(HERE))
import test_proposed,test_full_game_proposed

def load_tests(loader,tests,pattern):
 tests.addTests(loader.loadTestsFromModule(test_proposed))
 tests.addTests(loader.loadTestsFromModule(test_full_game_proposed))
 return tests
