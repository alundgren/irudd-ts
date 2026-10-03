"""Protect review suggestions from incorrect Stryker report source coordinates."""
import json
import unittest
from pathlib import Path
from summarize import original_text


class OriginalTextTests(unittest.TestCase):
    def test_recorded_boundary_and_boolean_are_exact(self):
        report = json.loads((Path(__file__).parent/'evidence/weak-stryker.json').read_text())['report']
        entry = report['files']['src/account.ts']
        by_name = {m['mutatorName']: m for m in entry['mutants'] if m['status']=='Survived'}
        self.assertEqual(original_text(entry['source'],by_name['EqualityOperator']['location']), 'total >= 100')
        self.assertEqual(original_text(entry['source'],by_name['BooleanLiteral']['location']), 'true')

    def test_multiline_expression_retains_newline_and_indentation(self):
        source = 'return (\n  first +\n  second\n);\n'
        location = {'start': {'line': 2, 'column': 3}, 'end': {'line': 3, 'column': 9}}
        self.assertEqual(original_text(source,location),'first +\n  second')

    def test_unicode_columns_count_utf16_units(self):
        source = "const label = '😀'; return value + 0;\n"
        prefix = source[:source.index('value')]
        column = len(prefix.encode('utf-16-le'))//2 + 1
        location = {'start':{'line':1,'column':column},'end':{'line':1,'column':column+9}}
        self.assertEqual(original_text(source,location),'value + 0')

    def test_invalid_location_does_not_return_plausible_text(self):
        with self.assertRaises(ValueError):
            original_text('return true;\n', {'start':{'line':1,'column':0},'end':{'line':1,'column':7}})
        with self.assertRaises(ValueError):
            original_text('return true;\n', {'start':{'line':1,'column':8},'end':{'line':1,'column':5}})


if __name__ == '__main__': unittest.main()
