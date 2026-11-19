"""scripts/generate_sbom.py: SPDX 2.3 SBOM of the crates a release binary is built from, from `cargo metadata`."""
import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import generate_sbom  # noqa: E402

REGISTRY = 'registry+https://github.com/rust-lang/crates.io-index'


def package(name, version='1.0.0', license=None, source=REGISTRY):
    return {'id': f'{name} {version}', 'name': name, 'version': version, 'license': license, 'source': source}


def node(name, deps=(), version='1.0.0'):
    return {'id': f'{name} {version}', 'deps': [{'pkg': f'{d} 1.0.0', 'dep_kinds': [{'kind': k}]} for d, k in deps]}


def metadata():
    """root -> a (normal), b (dev only); a -> c (build); c -> d (normal)."""
    return {
        'packages': [package('root', license='MIT', source=None), package('a', license='MIT/Apache-2.0'),
                     package('b', license='GPL-3.0-only'), package('c', license='Apache-2.0 OR MIT'),
                     package('d')],
        'resolve': {'root': 'root 1.0.0', 'nodes': [
            node('root', [('a', None), ('b', 'dev')]), node('a', [('c', 'build')]), node('c', [('d', None)]),
            node('b'), node('d')]},
    }


CHECKSUMS = {('a', '1.0.0'): 'aa' * 32, ('c', '1.0.0'): 'cc' * 32, ('d', '1.0.0'): 'dd' * 32}


def build(**kwargs):
    return generate_sbom.build_sbom(metadata(), CHECKSUMS, created='2026-01-01T00:00:00Z', **kwargs)


class Sbom(unittest.TestCase):
    def names(self, doc):
        return sorted(p['name'] for p in doc['packages'])

    def test_it_lists_the_crates_the_binary_is_built_from_and_not_dev_only_ones(self):
        self.assertEqual(self.names(build()), ['a', 'c', 'd', 'root'])

    def test_the_document_describes_the_root_package_and_links_dependencies(self):
        doc = build()
        self.assertEqual(doc['spdxVersion'], 'SPDX-2.3')
        self.assertEqual(doc['SPDXID'], 'SPDXRef-DOCUMENT')
        ids = {p['name']: p['SPDXID'] for p in doc['packages']}
        rels = {(r['spdxElementId'], r['relationshipType'], r['relatedSpdxElement']) for r in doc['relationships']}
        self.assertIn(('SPDXRef-DOCUMENT', 'DESCRIBES', ids['root']), rels)
        self.assertIn((ids['root'], 'DEPENDS_ON', ids['a']), rels)
        self.assertIn((ids['a'], 'DEPENDS_ON', ids['c']), rels)
        self.assertIn((ids['c'], 'DEPENDS_ON', ids['d']), rels)
        self.assertEqual(len(rels), 4, 'no edge to the dev-only crate')

    def test_each_package_carries_a_purl_a_download_location_and_the_lockfile_checksum(self):
        pkg = {p['name']: p for p in build()['packages']}
        a = pkg['a']
        self.assertEqual(a['downloadLocation'], 'https://crates.io/api/v1/crates/a/1.0.0/download')
        self.assertEqual(a['checksums'], [{'algorithm': 'SHA256', 'checksumValue': 'aa' * 32}])
        self.assertEqual(a['externalRefs'], [{'referenceCategory': 'PACKAGE-MANAGER', 'referenceType': 'purl',
                                             'referenceLocator': 'pkg:cargo/a@1.0.0'}])
        self.assertEqual(pkg['root']['downloadLocation'], 'NOASSERTION')
        self.assertNotIn('checksums', pkg['root'], 'a workspace crate has no registry checksum')

    def test_licenses_are_spdx_expressions_or_noassertion(self):
        pkg = {p['name']: p['licenseDeclared'] for p in build()['packages']}
        self.assertEqual(pkg['root'], 'MIT')
        self.assertEqual(pkg['a'], 'MIT OR Apache-2.0', 'the old `MIT/Apache-2.0` form means OR')
        self.assertEqual(pkg['c'], 'Apache-2.0 OR MIT')
        self.assertEqual(pkg['d'], 'NOASSERTION', 'no declared license is not guessed')
        self.assertEqual(generate_sbom.spdx_license('MIT AND (Apache-2.0 OR BSD-3-Clause)'),
                         'MIT AND (Apache-2.0 OR BSD-3-Clause)')
        self.assertEqual(generate_sbom.spdx_license('see LICENSE.txt; whatever'), 'NOASSERTION')

    def test_the_output_is_deterministic_and_has_unique_valid_ids(self):
        first, second = build(), build()
        self.assertEqual(json.dumps(first, sort_keys=True), json.dumps(second, sort_keys=True))
        ids = [p['SPDXID'] for p in first['packages']]
        self.assertEqual(len(ids), len(set(ids)))
        self.assertTrue(all(i.startswith('SPDXRef-') and all(c.isalnum() or c in '.-' for c in i[8:]) for i in ids))
        self.assertEqual(generate_sbom.problems(first), [])

    def test_a_package_the_lockfile_has_no_checksum_for_is_reported_not_invented(self):
        doc = generate_sbom.build_sbom(metadata(), {}, created='2026-01-01T00:00:00Z')
        a = next(p for p in doc['packages'] if p['name'] == 'a')
        self.assertNotIn('checksums', a)

    def test_the_validator_rejects_a_dangling_relationship_and_duplicate_ids(self):
        doc = build()
        doc['relationships'].append({'spdxElementId': 'SPDXRef-Package-x', 'relationshipType': 'DEPENDS_ON',
                                     'relatedSpdxElement': 'SPDXRef-Package-y'})
        self.assertTrue(any('SPDXRef-Package-x' in p for p in generate_sbom.problems(doc)))
        doc = build()
        doc['packages'].append(dict(doc['packages'][0]))
        self.assertTrue(any('duplicate' in p for p in generate_sbom.problems(doc)))

    def test_lockfile_checksums_are_read_from_cargo_lock_text(self):
        lock = '[[package]]\nname = "a"\nversion = "1.0.0"\nsource = "registry+x"\nchecksum = "' + 'ab' * 32 + '"\n\n' \
               '[[package]]\nname = "root"\nversion = "1.0.0"\n'
        self.assertEqual(generate_sbom.lock_checksums(lock), {('a', '1.0.0'): 'ab' * 32})


if __name__ == '__main__':
    unittest.main()
