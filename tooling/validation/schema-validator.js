#!/usr/bin/env node

/**
 * Nexus Schema Validator
 *
 * Validates all JSON Schema files in schemas/ directory against:
 * 1. JSON Schema Draft-07 specification
 * 2. Nexus meta schema requirements ($id, schema_version, etc.)
 */

const Ajv = require('ajv');
const addFormats = require('ajv-formats');
const fs = require('fs');
const path = require('path');

// Initialize AJV validator
const ajv = new Ajv({ strict: true, allErrors: true });
addFormats(ajv);

// Meta schema requirements
const META_SCHEMA = {
  type: 'object',
  required: ['$schema', '$id', 'schema_version', 'title', 'type'],
  properties: {
    $schema: { const: 'http://json-schema.org/draft-07/schema#' },
    $id: { type: 'string', format: 'uri' },
    schema_version: { type: 'integer', minimum: 1 },
    title: { type: 'string', minLength: 1 },
    type: { type: 'string' }
  }
};

function findSchemaFiles(dir) {
  const files = [];
  const items = fs.readdirSync(dir, { withFileTypes: true });
  
  for (const item of items) {
    const fullPath = path.join(dir, item.name);
    if (item.isDirectory()) {
      files.push(...findSchemaFiles(fullPath));
    } else if (item.isFile() && item.name.endsWith('.schema.json')) {
      files.push(fullPath);
    }
  }
  
  return files;
}

function validateSchema(filePath) {
  const relPath = path.relative(path.join(__dirname, '..', '..'), filePath);
  console.log(`Validating: ${relPath}`);
  
  try {
    const content = fs.readFileSync(filePath, 'utf8');
    const schema = JSON.parse(content);
    
    // Check meta requirements
    const metaValid = ajv.validate(META_SCHEMA, schema);
    if (!metaValid) {
      console.error(`  ❌ Meta validation failed:`);
      console.error(ajv.errorsText(ajv.errors));
      return false;
    }
    
    // Validate against JSON Schema Draft-07
    const valid = ajv.validateSchema(schema);
    if (!valid) {
      console.error(`  ❌ Schema syntax validation failed:`);
      console.error(ajv.errorsText(ajv.errors));
      return false;
    }
    
    console.log(`  ✓ Valid`);
    return true;
    
  } catch (err) {
    console.error(`  ❌ Error: ${err.message}`);
    return false;
  }
}

function main() {
  const schemasDir = path.join(__dirname, '..', '..', 'schemas');
  
  if (!fs.existsSync(schemasDir)) {
    console.error('schemas/ directory not found');
    process.exit(1);
  }
  
  console.log('Nexus Schema Validator');
  console.log('======================');
  console.log('');
  
  const schemaFiles = findSchemaFiles(schemasDir);
  
  if (schemaFiles.length === 0) {
    console.log('No schema files found');
    process.exit(0);
  }
  
  console.log(`Found ${schemaFiles.length} schema files`);
  console.log('');
  
  let validCount = 0;
  let invalidCount = 0;
  const invalidFiles = [];
  
  for (const file of schemaFiles) {
    if (validateSchema(file)) {
      validCount++;
    } else {
      invalidCount++;
      invalidFiles.push(file);
    }
  }
  
  console.log('');
  console.log('Summary:');
  console.log(`  Valid: ${validCount}`);
  console.log(`  Invalid: ${invalidCount}`);
  
  if (invalidCount > 0) {
    console.error('\n❌ Validation failed');
    process.exit(1);
  } else {
    console.log('\n✓ All schemas valid');
    process.exit(0);
  }
}

main();
