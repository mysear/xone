#!/usr/bin/env node
'use strict';
// Resolve from this wrapper, independently of the caller's cwd.
const path=require('node:path');
const ROOT=path.resolve(__dirname,'../../..');
const I=require(path.join(ROOT,'integration/tools/reference/balancer-managed-v2-shared/inputs.cjs'));
module.exports={main:I.main};
if(require.main===module)I.main().catch(I.fail);
