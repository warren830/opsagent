#!/usr/bin/env python3
"""
Entry point for EOS skill — can be called directly without PYTHONPATH.
Usage:
  python3 run.py list-regions --accounts 123456789012
  python3 run.py scan --accounts 123456789012 --regions us-east-1
"""
import sys
import os

# Add the skill directory to path so eos_skill package is importable
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from eos_skill.main import main

if __name__ == "__main__":
    main()
