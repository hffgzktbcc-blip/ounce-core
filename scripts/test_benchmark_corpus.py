import unittest

from benchmark_corpus import summarize


class BenchmarkTests(unittest.TestCase):
    def test_rates_use_runner_time_and_guest_work(self):
        samples = [{'instructions': 100, 'queued': [1, 2], 'wall_seconds': s} for s in (2, 4, 1)]
        report = summarize(samples)
        self.assertEqual(report['instructions_per_second_median'], 50)
        self.assertEqual(report['instructions_per_second_min'], 25)
        self.assertEqual(report['instructions_per_second_max'], 100)
        self.assertEqual(report['frames_per_second_median'], 1)

    def test_rejects_changed_guest_work(self):
        sample = {'instructions': 100, 'queued': [1], 'wall_seconds': 2}
        for changed in ({'instructions': 101}, {'queued': [2]}):
            with self.assertRaises(ValueError):
                summarize([sample, dict(sample, **changed)])


if __name__ == '__main__':
    unittest.main()
