# Security policy

Please report a vulnerability privately through GitHub's "Report a vulnerability" option on the repository's Security tab, not in a public issue. Include the affected version or commit, how to reproduce, and the impact. You should get an acknowledgement within a week.

Ounce-Core runs untrusted guest code. Anything that lets a guest program read or write the host outside its sandbox (the `--sdmc` directory), crash the host, or consume unbounded host resources is in scope. A guest program computing a wrong result is a normal bug: use the issue tracker.

This is an experimental research project; there are no supported releases with security backports.
