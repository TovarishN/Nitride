import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from parser_census import aggregate, split_sources, verdict  # noqa: E402


def decision(kind, entries, single, attempts=None, consumed=0, name="R", empty=0):
    return {
        "kind": kind,
        "rule": 0,
        "rule_name": name,
        "ordinal": 0,
        "alternatives": 2,
        "entries": entries,
        "single": single,
        "empty": empty,
        "attempts": entries if attempts is None else attempts,
        "failed_attempts": 0,
        "consumed_failures": consumed,
        "unmatched": 0,
    }


def report(source_bytes, decisions, predicates=0, repeat_consumed=0, path="target/parser-compare/4m-valid.cs"):
    return {
        "path": path,
        "source_bytes": source_bytes,
        "decisions": decisions,
        "predicate_evaluations": predicates,
        "context_entries": 0,
        "repeat_exits": 0,
        "repeat_consumed_exits": repeat_consumed,
    }


class AggregateTest(unittest.TestCase):
    def test_aggregate_shares_and_rates(self):
        summary = aggregate([
            report(1000, [decision("choice", 10, 9, consumed=10), decision("pratt-prefix", 5, 5)], predicates=1),
            report(1000, [decision("choice", 10, 8, consumed=10)], predicates=3, repeat_consumed=10),
        ])
        self.assertEqual(summary["source_bytes"], 2000)
        self.assertEqual(summary["choice_entries"], 20)
        self.assertAlmostEqual(summary["choice_single_share"], 0.85)
        self.assertEqual(summary["pratt_entries"], 5)
        self.assertAlmostEqual(summary["pratt_single_share"], 1.0)
        self.assertAlmostEqual(summary["consumed_failures_per_kb"], 15.0)
        self.assertAlmostEqual(summary["predicates_per_kb"], 2.0)

    def test_shares_count_zero_attempt_entries_as_decided(self):
        summary = aggregate([report(1000, [
            decision("choice", 10, 6, empty=3),
            decision("pratt-infix", 10, 2, attempts=2, empty=8),
        ])])
        self.assertAlmostEqual(summary["choice_single_share"], 0.9)
        self.assertAlmostEqual(summary["pratt_single_share"], 1.0)

    def test_exactly_one_share_excludes_zero_attempt_entries(self):
        summary = aggregate([report(1000, [decision("choice", 10, 6, empty=3)])])
        self.assertAlmostEqual(summary["choice_exactly_one_share"], 0.6)
        self.assertAlmostEqual(summary["choice_single_share"], 0.9)

    def test_split_sources_separates_generated_input_and_corpus(self):
        generated = report(4000, [decision("choice", 10, 10)])
        corpus = report(1000, [decision("choice", 10, 5)], path="languages/csharp4/corpus/Mono2004/Boolean.cs")
        groups = split_sources([generated, corpus])
        self.assertEqual(sorted(groups), ["corpus", "generated"])
        self.assertEqual(groups["generated"]["source_bytes"], 4000)
        self.assertAlmostEqual(groups["corpus"]["choice_single_share"], 0.5)

    def test_top_ranks_by_extra_attempts(self):
        summary = aggregate([report(1000, [
            decision("choice", 10, 10, attempts=10, name="flat"),
            decision("choice", 10, 2, attempts=30, name="worst"),
            decision("pratt-infix", 10, 5, attempts=15, name="middle"),
        ])])
        self.assertEqual([d["rule_name"] for d in summary["top"]], ["worst", "middle", "flat"])


class VerdictTest(unittest.TestCase):
    def summary(self, choice=0.95, pratt=0.95, consumed=1.0, predicates=0.0):
        return {
            "choice_single_share": choice,
            "pratt_single_share": pratt,
            "consumed_failures_per_kb": consumed,
            "predicates_per_kb": predicates,
        }

    def test_verdict_thresholds(self):
        self.assertTrue(verdict(self.summary(choice=0.90))["g1"])
        self.assertFalse(verdict(self.summary(choice=0.899))["g1"])
        self.assertTrue(verdict(self.summary(pratt=0.90))["g2"])
        self.assertFalse(verdict(self.summary(pratt=0.899))["g2"])
        self.assertTrue(verdict(self.summary(consumed=20.0))["g3"])
        self.assertFalse(verdict(self.summary(consumed=20.01))["g3"])
        self.assertFalse(verdict(self.summary(predicates=5.0))["predicate_support"])
        self.assertTrue(verdict(self.summary(predicates=5.01))["predicate_support"])
        self.assertTrue(verdict(self.summary())["go"])
        self.assertFalse(verdict(self.summary(consumed=21.0))["go"])
        self.assertTrue(verdict(self.summary(predicates=9.0))["go"])


if __name__ == "__main__":
    unittest.main()
