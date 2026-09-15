#!/usr/bin/env node
import { nativeCompatibility } from '@42ch/nexus-native';

const compat = nativeCompatibility();
console.log(JSON.stringify({ target: compat.target_triple, hash: compat.contract_tree_sha256 }));
