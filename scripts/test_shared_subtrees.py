"""Measurement tests: duplicate reader references must not double physical bytes."""
import unittest
from unittest import mock
from types import SimpleNamespace
import pathlib,tempfile,json,contextlib,io
from shared_subtrees import unique_storage_bytes, percentile, summarize_edits, cold_gate_failures, roundtrip_edits, measure

class Measurements(unittest.TestCase):
    def test_unique_shared_bytes_counted_once(self):
        self.assertEqual(unique_storage_bytes([[(1, 64), (2, 32)], [(1, 64), (3, 16)]]), 112)

    def test_inconsistent_chunk_size_is_rejected(self):
        with self.assertRaises(ValueError):
            unique_storage_bytes([[(1, 64)], [(1, 32)]])

    def test_percentiles_include_tail(self):
        self.assertEqual(percentile(list(range(1, 101)), 95), 95)
        self.assertEqual(percentile([9], 95), 9)
        with self.assertRaises(ValueError):
            percentile([], 95)

class EditTraces(unittest.TestCase):
    def test_utf8_inverse_edits_restore_the_actual_previous_bytes(self):
        edits,changed=roundtrip_edits("a😀b",[{"start":1,"end":5,"replacement":"λ"},{"start":3,"end":4,"replacement":"!"}])
        self.assertEqual(changed,"aλ!")
        raw="a😀b".encode()
        for edit in edits:
            a,b=edit["range"];raw=raw[:a]+edit["replacement"].encode()+raw[b:]
        self.assertEqual(raw.decode(),"a😀b")

class Admission(unittest.TestCase):
    def test_warmups_are_excluded_and_fallbacks_remain_visible(self):
        def row(ns, fallback, copied):
            return {"phase":{"elapsed_ns":ns},"snapshot":{"stats":{"full_parse_fallbacks":fallback,"syntax_bytes_copied":copied,"source_bytes_copied":1}},"equivalent":True}
        result=summarize_edits({"edits":[row(100000,0,0),row(100,0,16),row(200,1,None),row(300,0,32)]},1)
        self.assertEqual((result["p50_ns"],result["p95_ns"],result["max_ns"]),(200,300,300))
        self.assertEqual(result["fallbacks"],1)
        self.assertEqual(result["unknown_copy_samples"],1)

    def test_warmup_copy_fallback_and_equivalence_failures_are_not_hidden(self):
        def row(ns, copied, fallback, equivalent):
            return {"phase":{"elapsed_ns":ns},"snapshot":{"stats":{"full_parse_fallbacks":fallback,"syntax_bytes_copied":copied,"source_bytes_copied":1}},"equivalent":equivalent}
        result=summarize_edits({"edits":[row(100000,1_000_000,1,False),row(100,0,0,True)]},1)
        self.assertEqual(result["p95_ns"],100)
        self.assertEqual(result["max_copied_bytes"],1_000_001)
        self.assertEqual(result["fallbacks"],1)
        self.assertEqual(result["equivalence_failures"],1)
        self.assertEqual(result["warmup_max_ns"],100000)

    def test_equivalence_mismatch_fails_measurement_and_is_retained(self):
        phase={"elapsed_ns":100,"peak_bytes":100,"retained_bytes":100,"allocation_instrumented":True}
        edit={"phase":{"elapsed_ns":1},"equivalent":False,"snapshot":{"stats":{"full_parse_fallbacks":0,"syntax_bytes_copied":0,"source_bytes_copied":1}}}
        def run(command,**kwargs):
            result={"edits":[edit]} if "--edits" in command else {"parse":phase}
            return SimpleNamespace(returncode=0,stdout=json.dumps(result),stderr="")
        with tempfile.TemporaryDirectory() as folder:
            root=pathlib.Path(folder);profiler=root/"profiler";profiler.write_text("")
            baseline=root/"baseline.json";baseline.write_text(json.dumps({"measurements":[{"profile":"csharp4","size_target":262144,"pattern":"strict","samples":[{"result":{"parse":phase}}]}]}))
            args=SimpleNamespace(profiler=profiler,baseline_report=baseline,report=root/"report.json",sizes=[262144],patterns=[],runs=1,warmups=0,edits=1,locations=["middle"],readers=[0],validate=True,latency_only=False)
            with mock.patch("shared_subtrees.subprocess.run",side_effect=run),mock.patch("shared_subtrees.source_digest",return_value="test"),mock.patch("shared_subtrees.platform.platform",return_value="test"),mock.patch("shared_subtrees.platform.machine",return_value="test"),contextlib.redirect_stdout(io.StringIO()):
                status=measure(args)
            report=json.loads(args.report.read_text())
            self.assertEqual(status,1)
            self.assertTrue(any(f["kind"]=="equivalence" for f in report["failures"]))

    def test_uninstrumented_measurement_keeps_allocator_metrics_unavailable(self):
        phase={"elapsed_ns":100,"peak_bytes":None,"retained_bytes":None,"allocation_instrumented":False}
        edit={"phase":phase,"equivalent":True,"snapshot":{"stats":{"full_parse_fallbacks":0,"syntax_bytes_copied":0,"source_bytes_copied":1}}}
        def run(command,**kwargs):
            result={"edits":[edit]} if "--edits" in command else {"parse":phase}
            return SimpleNamespace(returncode=0,stdout=json.dumps(result),stderr="")
        with tempfile.TemporaryDirectory() as folder:
            root=pathlib.Path(folder);profiler=root/"profiler";profiler.write_text("")
            args=SimpleNamespace(profiler=profiler,baseline_report=root/"absent.json",report=root/"report.json",sizes=[262144],patterns=[],runs=1,warmups=0,edits=1,locations=["middle"],readers=[0],validate=True,latency_only=True)
            with mock.patch("shared_subtrees.subprocess.run",side_effect=run),mock.patch("shared_subtrees.source_digest",return_value="test"),mock.patch("shared_subtrees.platform.platform",return_value="test"),mock.patch("shared_subtrees.platform.machine",return_value="test"),contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(measure(args),1)  # 100 ns edits exceed 20% of 100 ns cold.
            report=json.loads(args.report.read_text())
            self.assertTrue(report["latency_only"])
            self.assertEqual(report["measurements"][0]["legacy_cold"][0]["parse"]["peak_bytes"],None)
            self.assertTrue(all(f.get("metric","elapsed_ns")=="elapsed_ns" for f in report["failures"]))

    def test_failed_numeric_gates_are_kept(self):
        baseline={"elapsed_ns":100,"peak_bytes":100,"retained_bytes":100}
        self.assertEqual(len(cold_gate_failures(baseline,{"elapsed_ns":126,"peak_bytes":151,"retained_bytes":136})),3)
        self.assertEqual(cold_gate_failures(baseline,{"elapsed_ns":125,"peak_bytes":150,"retained_bytes":135}),[])

if __name__ == "__main__":
    unittest.main()
