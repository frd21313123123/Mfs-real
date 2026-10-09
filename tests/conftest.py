from pathlib import Path
import pytest
from realflow.airport import AirportGraph
from realflow.fsltl import AircraftModel,ModelMatcher

@pytest.fixture
def graph():
    return AirportGraph.from_json(Path(__file__).parent.parent/'examples'/'demo_airport.json')

@pytest.fixture
def matcher():
    return ModelMatcher([
        AircraftModel('FSLTL Airbus A320 AFL','A320','AFL','fixture'),
        AircraftModel('FSLTL Boeing B738 UAL','B738','UAL','fixture'),
        AircraftModel('FSLTL Airbus A320 DLH','A320','DLH','fixture')])
