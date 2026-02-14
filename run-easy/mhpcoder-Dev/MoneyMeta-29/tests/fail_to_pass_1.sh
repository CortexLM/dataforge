#!/bin/bash
# This test must FAIL on base commit, PASS after fix
node -e 'const {execSync}=require("child_process");let out="";try{out=execSync("rg -n \"MoneyMeta|moneymeta\" src/app src/components",{stdio:["ignore","pipe","pipe"]}).toString();}catch(err){out=(err.stdout||"").toString();}if(out.trim().length){console.error("Found MoneyMeta references:\n"+out);process.exit(1);}console.log("No MoneyMeta references found");'
