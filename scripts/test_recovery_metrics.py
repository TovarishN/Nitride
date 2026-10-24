"""Hand-checked source-mapping and preservation contracts."""
import unittest
from recovery_corpus import ROOT, Edit, map_span, preservation, inverse_span, semantic_items
import json
import subprocess
from recovery_compare import quality_failures


class MappingTests(unittest.TestCase):
    def test_insertion_moves_following_item_but_not_previous(self):
        edit = Edit(3, 3, "𝑥")
        self.assertEqual(map_span([0, 3], edit), [0, 3])
        self.assertEqual(map_span([3, 7], edit), [7, 11])
        self.assertIsNone(map_span([2, 6], edit))

    def test_deletion_excludes_only_overlapping_units(self):
        edit = Edit(4, 7, "")
        self.assertEqual(map_span([0, 4], edit), [0, 4])
        self.assertEqual(map_span([7, 12], edit), [4, 9])
        self.assertIsNone(map_span([3, 8], edit))

    def test_preservation_checks_structure_and_identity_not_names(self):
        before = [{"span": [0, 3], "shape": "a"}, {"span": [4, 7], "shape": "b"}, {"span": [8, 11], "shape": "b"}]
        after = [{"span": [0, 3], "shape": "wrong"}, {"span": [7, 10], "shape": "b"}, {"span": [11, 14], "shape": "b"}]
        self.assertEqual(preservation(before, after, Edit(3, 3, "xxx")), {"eligible": 3, "preserved": 2})


class SemanticMappingTests(unittest.TestCase):
    def test_unchanged_hir_binding_and_origins_survive_inserted_utf8(self):
        def observation(delta):
            return {"nodes":[{"span":[8+delta,12+delta],"item":True}],
                "hir":[{"id":0,"kind":"reference","type":"Scalar","span":[8+delta,11+delta],"symbol":2,"args":[],"origins":[[8+delta,11+delta],[100,104]],"origin_rewrites":[False,True]}],
                "bindings":[{"namespace":"value","span":[8+delta,11+delta],"symbol":2}],"statements":[{"value":0}]}
        before = semantic_items(observation(0))
        edit = Edit(5,5,"😀")
        after = semantic_items(observation(4), edit)
        self.assertEqual(preservation(before,after,edit),{"eligible":1,"preserved":1})
        damaged = observation(4)
        damaged["hir"][0]["type"] = "<error>"
        self.assertEqual(preservation(before,semantic_items(damaged,edit),edit)["preserved"],0)

    def test_nested_items_and_zero_width_edits_use_source_identity(self):
        edit = Edit(4,4,"x")
        self.assertIsNone(map_span([0,10],edit))
        self.assertEqual(map_span([0,4],edit),[0,4])
        self.assertEqual(map_span([4,8],edit),[5,9])
        self.assertEqual(inverse_span([5,9],edit),[4,8])
        self.assertIsNone(inverse_span([3,6],edit))

    def test_limit_diagnostics_count_toward_the_roslyn_cascade_gate(self):
        failures = quality_failures({"csharp4:insert": {"coverage_failures":0,
            "paired_native_preserved":1,"paired_native_eligible":1,"paired_roslyn_preserved":1,"paired_roslyn_eligible":1,
            "native_diagnostics":0,"native_total_diagnostics":2,"roslyn_diagnostics":1}})
        self.assertEqual(len(failures),1)
        self.assertIn("2 diagnostics",failures[0])

    def test_roslyn_offsets_are_utf8_after_supplementary_scalar(self):
        source = 'class C { string s = "😀"; int x = ; }'
        result = subprocess.run(["dotnet",str(ROOT/"tools/Nitride.RecoveryReference/bin/Debug/net10.0/Nitride.RecoveryReference.dll")],
            input=json.dumps({"source":source})+"\n",capture_output=True,text=True,check=True)
        observation = json.loads(result.stdout)
        at = source.encode().index(b';',source.encode().index(b'int x'))
        self.assertEqual(observation["diagnostics"][0]["span"],[at,at+1])
        token = next(t for t in observation["tokens"] if '😀' in t["text"])
        self.assertEqual(source.encode()[slice(*token["span"])].decode(),'"😀"')


if __name__ == "__main__":
    unittest.main()
