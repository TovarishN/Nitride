"""Freeze/check actual Nitrogen functionsd binding, types, HIR and execution."""
import json
import pathlib
import subprocess
import sys
import tempfile
from test_reference import ROOT, run


def observe(source):
    with tempfile.TemporaryDirectory() as folder:
        path=pathlib.Path(folder)/"input.functions"
        path.write_text(source)
        result=run("oracle-functions",path,ROOT/"languages/functions/Functions.ncatalog")
        assert result.returncode==0,result.stderr
        return json.loads(result.stdout)


def main():
    frozen=[]
    for case in json.loads((ROOT/"fixtures/functions-cases.json").read_text()):
        result=observe(case["source"])
        if "error" in case:
            assert not result["ok"] and result["error"]==case["error"],(case,result)
            assert result["trace"]==[]
        else:
            assert result["ok"] and result["values"]==case["values"],(case,result)
        frozen.append({"name":case["name"],"source":case["source"],"observation":result})
    path=ROOT/"fixtures/functions-reference.json"
    if "--freeze" in sys.argv:
        path.write_text(json.dumps(frozen,indent=2,ensure_ascii=False)+"\n")
        print(f"Frozen {len(frozen)} functions cases from real Nitrogen")
    else:
        assert json.loads(path.read_text())==frozen,"frozen functions reference changed"
        for case in frozen:
            with tempfile.TemporaryDirectory() as folder:
                source=pathlib.Path(folder)/"input.functions";source.write_text(case["source"])
                result=subprocess.run([ROOT/"target/release/nitride","--functions",source],capture_output=True,text=True)
                actual=json.loads(result.stdout);expected=case["observation"].copy()
                actual.pop("message",None);expected.pop("message",None)
                assert actual==expected,(case["name"],expected,actual)
        print(f"{len(frozen)} functions cases match live Nitrogen and native release")
        with tempfile.TemporaryDirectory() as folder:
            source_path=pathlib.Path(folder)/"input.functions"
            for index in range(64):
                value=index%8+1
                text=f"def sum(a: Scalar,b: Scalar)=add(a,b); def twice(x: Scalar)=call sum(x,x); call twice(neg({value}));"
                if index%5==0:text+=" call sum(1);"
                elif index%7==0:text+=" call twice(duration(2));"
                elif index%11==0:text+=" def loop(x: Scalar)=call loop(x); call loop(1);"
                elif index%3==0:text+=" def drop(x: Scalar)=2; call drop(neg(3));"
                source_path.write_text(text)
                result=subprocess.run([ROOT/"target/release/nitride","--functions",source_path],capture_output=True,text=True)
                actual=json.loads(result.stdout);expected=observe(text)
                actual.pop("message",None);expected.pop("message",None)
                assert actual==expected,(index,text,expected,actual)
        print("64 generated functions programs match live Nitrogen")


if __name__=="__main__":main()
