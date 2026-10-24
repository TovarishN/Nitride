"""Freeze/check actual Nitrogen inferenced binding, types, HIR and execution."""
import json
import pathlib
import subprocess
import sys
import tempfile
from test_reference import ROOT, run


def observe(source):
    with tempfile.TemporaryDirectory() as folder:
        path=pathlib.Path(folder)/"input.inference"
        path.write_text(source)
        result=run("oracle-inference",path,ROOT/"languages/inference/catalog.json")
        assert result.returncode==0,result.stderr
        return json.loads(result.stdout)


def main():
    frozen=[]
    for case in json.loads((ROOT/"fixtures/inference-cases.json").read_text()):
        result=observe(case["source"])
        if "error" in case:
            assert not result["ok"] and result["error"]==case["error"],(case,result)
            assert result["trace"]==[]
        else:
            assert result["ok"] and result["values"]==case["values"],(case,result)
        frozen.append({"name":case["name"],"source":case["source"],"observation":result})
    path=ROOT/"fixtures/inference-reference.json"
    if "--freeze" in sys.argv:
        path.write_text(json.dumps(frozen,indent=2,ensure_ascii=False)+"\n")
        print(f"Frozen {len(frozen)} inference cases from real Nitrogen")
    else:
        assert json.loads(path.read_text())==frozen,"frozen inference reference changed"
        for case in frozen:
            with tempfile.TemporaryDirectory() as folder:
                source=pathlib.Path(folder)/"input.inference";source.write_text(case["source"])
                result=subprocess.run([ROOT/"target/release/nitride","--inference",source],capture_output=True,text=True)
                actual=json.loads(result.stdout);expected=case["observation"].copy()
                actual.pop("message",None);expected.pop("message",None)
                assert actual==expected,(case["name"],expected,actual)
        print(f"{len(frozen)} inference cases match live Nitrogen and native release")
        with tempfile.TemporaryDirectory() as folder:
            source_path=pathlib.Path(folder)/"input.inference"
            for index in range(64):
                value=index % 8 + 1
                declarations=[f"let x={value};", f"rate x={value+10};"]
                if index % 3 == 0: declarations.reverse()
                if index % 5 == 0: declarations=declarations[:1]
                text=" ".join(declarations)
                if index % 4 == 0: text += " let x=either(x)+1;"
                if index % 7 == 0: text += " rate x=duration(3);"
                ref=["value","rate","either","reverse"][index % 4]
                text += f" {ref}(x){'+1' if index % 6 == 0 else ''};"
                if index % 11 == 0: text += " value(missing);"
                source_path.write_text(text)
                result=subprocess.run([ROOT/"target/release/nitride","--inference",source_path],capture_output=True,text=True)
                actual=json.loads(result.stdout);expected=observe(text)
                actual.pop("message",None);expected.pop("message",None)
                assert actual==expected,(index,text,expected,actual)
        print("64 generated inference programs match live Nitrogen")


if __name__=="__main__":main()
