import json
import os
from pathlib import Path
import subprocess
import sys
import pytest
from realflow.config import Settings
from realflow.launcher import launch_command, save_profile


def test_profile_roundtrip_and_invalid_write_preserves_existing(tmp_path):
    profile = tmp_path / 'profile.json'
    settings = Settings(mode='simulation', airborne_limit=3, ground_limit=2,
                        fsltl_path='models with spaces', airport_graph='airport.json')
    save_profile(settings, profile)
    assert Settings.load(profile) == settings
    original = profile.read_bytes()
    settings.airborne_limit = 36
    with pytest.raises(ValueError):
        save_profile(settings, profile)
    assert profile.read_bytes() == original
    assert list(tmp_path.iterdir()) == [profile]


def test_launcher_demo_uses_selected_profile(tmp_path):
    profile = tmp_path / 'profile with spaces.json'
    output = tmp_path / 'output.json'
    save_profile(Settings(mode='simulation', airborne_limit=2, ground_limit=1), profile)
    command = launch_command(Settings.load(profile), profile, 'demo', output)
    result = subprocess.run(command, cwd=tmp_path, capture_output=True, text=True)
    assert result.returncode == 0, result.stderr
    report = json.loads(output.read_text())
    assert report['mode'] == 'simulation'
    assert report['limits'] == {'air': 2, 'ground': 1}
    assert report['clean_shutdown']
    assert report['events']['creates'] > 0


def test_packaged_launcher_requires_sibling_cli(tmp_path, monkeypatch):
    monkeypatch.setattr(sys, 'frozen', True, raising=False)
    monkeypatch.setattr(sys, 'executable', str(tmp_path / 'RealFlowLauncher.exe'))
    with pytest.raises(ValueError, match='RealFlowTraffic-ATC.exe'):
        launch_command(Settings(), tmp_path / 'profile.json', 'demo', tmp_path / 'out.json')
    cli = tmp_path / 'RealFlowTraffic-ATC.exe'
    cli.touch()
    assert launch_command(Settings(), tmp_path / 'profile.json', 'demo', tmp_path / 'out.json')[0] == str(cli)


@pytest.mark.skipif(os.name == 'nt', reason='Linux platform guard')
def test_simulator_requires_windows(tmp_path):
    with pytest.raises(ValueError, match='Windows'):
        launch_command(Settings(), tmp_path / 'profile.json', 'simconnect', tmp_path / 'out.json')
