#!/usr/bin/env python3
"""
Event Parser for Stream Health Monitor Tests

Robust JSON event parsing that doesn't rely on brittle grep patterns.
Provides canonical event taxonomy and assertion helpers.

Canonical Event Types:
    alert.silence       - Sustained silence detected
    alert.low_volume    - Audio below volume threshold
    alert.clipping      - Audio distortion/saturation
    alert.channel_imbalance - L/R channel imbalance
    alert.dropouts      - Intermittent silence bursts
    alert.freeze        - Video/timing freeze (timing-based)
    alert.drift         - Timing drift (timing-based)
    alert.keyword       - Keyword detected (future)
    health              - Periodic health score

Usage:
    # Parse events from demo output
    python parse_events.py < demo_output.txt
    
    # Use as module
    from parse_events import EventParser
    parser = EventParser(output_text)
    assert parser.has_alert('clipping')
    assert parser.count('alert.clipping') >= 1
"""

import json
import sys
from dataclasses import dataclass, field
from typing import Dict, List, Optional, Set
from collections import defaultdict


# Canonical event type mappings
# Maps runtime JSON "type" values to canonical alert types
TYPE_TO_CANONICAL = {
    # Audio content faults
    "silence": "alert.silence",
    "low_volume": "alert.low_volume",
    "clipping": "alert.clipping",
    "channel_imbalance": "alert.channel_imbalance",
    "dropouts": "alert.dropouts",
    
    # Timing-based faults
    "freeze": "alert.freeze",
    "drift": "alert.drift",
    "cadence": "alert.cadence",
    "av_skew": "alert.av_skew",
    
    # Health events
    "health": "health",
    
    # Future
    "keyword": "alert.keyword",
}


@dataclass
class ParsedEvent:
    """A parsed health event with canonical type"""
    canonical_type: str
    raw_type: str
    timestamp: Optional[str] = None
    data: Dict = field(default_factory=dict)
    
    def is_alert(self) -> bool:
        return self.canonical_type.startswith("alert.")
    
    def alert_name(self) -> Optional[str]:
        """Get just the alert name (e.g., 'clipping' from 'alert.clipping')"""
        if self.is_alert():
            return self.canonical_type.split(".", 1)[1]
        return None


class EventParser:
    """Parser for demo binary JSON output with robust event handling"""
    
    def __init__(self, output: str):
        self.raw_output = output
        self.events: List[ParsedEvent] = []
        self.parse_errors: List[str] = []
        self._parse()
    
    def _parse(self):
        """Parse JSON lines from output"""
        for line in self.raw_output.strip().split('\n'):
            line = line.strip()
            if not line:
                continue
            
            # Skip non-JSON lines (logs, etc.)
            if not line.startswith('{'):
                continue
            
            try:
                data = json.loads(line)
                event = self._parse_event(data)
                if event:
                    self.events.append(event)
            except json.JSONDecodeError as e:
                self.parse_errors.append(f"JSON error: {e} in line: {line[:100]}")
    
    def _parse_event(self, data: Dict) -> Optional[ParsedEvent]:
        """Convert a JSON object to a ParsedEvent"""
        event_type = data.get("type")
        if not event_type:
            return None
        
        canonical = TYPE_TO_CANONICAL.get(event_type, f"unknown.{event_type}")
        
        return ParsedEvent(
            canonical_type=canonical,
            raw_type=event_type,
            timestamp=data.get("ts"),
            data=data
        )
    
    def count(self, canonical_type: str) -> int:
        """Count events of a specific canonical type"""
        return sum(1 for e in self.events if e.canonical_type == canonical_type)
    
    def count_alerts(self) -> int:
        """Count all alert events"""
        return sum(1 for e in self.events if e.is_alert())
    
    def has_alert(self, alert_name: str) -> bool:
        """Check if a specific alert type was emitted"""
        canonical = f"alert.{alert_name}"
        return self.count(canonical) > 0
    
    def alert_types(self) -> Set[str]:
        """Get set of all alert names that were emitted"""
        return {e.alert_name() for e in self.events if e.is_alert()}
    
    def health_scores(self) -> List[float]:
        """Get all health scores emitted"""
        scores = []
        for e in self.events:
            if e.canonical_type == "health":
                score = e.data.get("score")
                if score is not None:
                    scores.append(float(score))
        return scores
    
    def summary(self) -> Dict[str, int]:
        """Get count of each canonical event type"""
        counts = defaultdict(int)
        for e in self.events:
            counts[e.canonical_type] += 1
        return dict(counts)
    
    def validate_no_unexpected_alerts(self, expected: Set[str], allowed_health: bool = True) -> List[str]:
        """
        Validate that only expected alert types are present.
        Returns list of validation errors.
        """
        errors = []
        for e in self.events:
            if e.is_alert():
                if e.alert_name() not in expected:
                    errors.append(f"Unexpected alert: {e.canonical_type}")
            elif e.canonical_type == "health" and not allowed_health:
                errors.append("Unexpected health event")
        return errors


@dataclass
class TestAssertion:
    """A test assertion with expected outcomes"""
    fault_type: str
    expected_alerts: Set[str]
    forbidden_alerts: Set[str]
    min_expected_count: int = 1
    
    def validate(self, parser: EventParser) -> List[str]:
        """Validate assertions against parsed events, return errors"""
        errors = []
        
        # Check expected alerts are present
        for alert in self.expected_alerts:
            count = parser.count(f"alert.{alert}")
            if count < self.min_expected_count:
                errors.append(
                    f"Expected at least {self.min_expected_count} {alert} events, got {count}"
                )
        
        # Check forbidden alerts are absent
        for alert in self.forbidden_alerts:
            count = parser.count(f"alert.{alert}")
            if count > 0:
                errors.append(
                    f"Forbidden alert {alert} was emitted {count} times"
                )
        
        return errors


# Pre-defined test assertions for each fault type
FAULT_ASSERTIONS = {
    "none": TestAssertion(
        fault_type="none",
        expected_alerts=set(),
        forbidden_alerts={"silence", "clipping", "low_volume", "channel_imbalance", "dropouts"},
    ),
    "silence": TestAssertion(
        fault_type="silence",
        expected_alerts={"silence"},
        forbidden_alerts={"clipping"},  # Silence shouldn't trigger clipping
    ),
    "low_volume": TestAssertion(
        fault_type="low_volume",
        expected_alerts={"low_volume"},
        forbidden_alerts={"clipping", "channel_imbalance"},
    ),
    "clipping": TestAssertion(
        fault_type="clipping",
        expected_alerts={"clipping"},
        forbidden_alerts={"silence", "low_volume"},  # Clipped audio isn't silent or quiet
    ),
    "channel_imbalance": TestAssertion(
        fault_type="channel_imbalance",
        expected_alerts={"channel_imbalance"},
        forbidden_alerts={"clipping"},
    ),
    "dropouts": TestAssertion(
        fault_type="dropouts",
        expected_alerts={"dropouts", "silence"},  # Dropouts may also emit silence
        forbidden_alerts={"clipping"},
        min_expected_count=1,
    ),
    "combined": TestAssertion(
        fault_type="combined",
        expected_alerts=set(),  # Combined can have multiple
        forbidden_alerts=set(),  # Anything goes
        min_expected_count=0,
    ),
    # Timing-based faults - marked as SKIP for WAV tests
    "drift": TestAssertion(
        fault_type="drift",
        expected_alerts=set(),  # WAV can't test timing
        forbidden_alerts=set(),
    ),
    "jitter": TestAssertion(
        fault_type="jitter",
        expected_alerts=set(),  # WAV can't test timing
        forbidden_alerts=set(),
    ),
}


def main():
    """CLI entry point for parsing events"""
    import argparse
    
    parser = argparse.ArgumentParser(description="Parse stream health monitor events")
    parser.add_argument("--input", "-i", type=str, help="Input file (default: stdin)")
    parser.add_argument("--fault", "-f", type=str, help="Expected fault type for validation")
    parser.add_argument("--summary", "-s", action="store_true", help="Print event summary")
    parser.add_argument("--json", "-j", action="store_true", help="Output as JSON")
    
    args = parser.parse_args()
    
    # Read input
    if args.input:
        with open(args.input) as f:
            output = f.read()
    else:
        output = sys.stdin.read()
    
    # Parse events
    event_parser = EventParser(output)
    
    # Print parse errors if any
    if event_parser.parse_errors:
        for err in event_parser.parse_errors:
            print(f"PARSE WARNING: {err}", file=sys.stderr)
    
    # Summary mode
    if args.summary or not args.fault:
        summary = event_parser.summary()
        if args.json:
            print(json.dumps({
                "total_events": len(event_parser.events),
                "alert_count": event_parser.count_alerts(),
                "by_type": summary,
                "health_scores": event_parser.health_scores(),
                "alert_types": list(event_parser.alert_types()),
            }, indent=2))
        else:
            print(f"Total events: {len(event_parser.events)}")
            print(f"Alert events: {event_parser.count_alerts()}")
            print(f"Alert types: {', '.join(sorted(event_parser.alert_types())) or 'none'}")
            print("\nBy type:")
            for event_type, count in sorted(summary.items()):
                print(f"  {event_type}: {count}")
    
    # Validation mode
    if args.fault:
        assertion = FAULT_ASSERTIONS.get(args.fault)
        if not assertion:
            print(f"Unknown fault type: {args.fault}", file=sys.stderr)
            sys.exit(2)
        
        errors = assertion.validate(event_parser)
        
        if args.json:
            print(json.dumps({
                "fault": args.fault,
                "passed": len(errors) == 0,
                "errors": errors,
                "summary": event_parser.summary(),
            }, indent=2))
        else:
            if errors:
                print(f"FAIL: {args.fault}")
                for err in errors:
                    print(f"  - {err}")
                sys.exit(1)
            else:
                print(f"PASS: {args.fault}")
                print(f"  Alerts: {', '.join(sorted(event_parser.alert_types())) or 'none'}")
        
        sys.exit(0 if not errors else 1)


if __name__ == "__main__":
    main()
