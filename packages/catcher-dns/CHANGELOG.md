# Changelog

## [0.4.0](https://github.com/eric8810/catcher/compare/catcher-dns-v0.3.13...catcher-dns-v0.4.0) (2026-06-14)


### Features

* **catcher-dns:** add clear_cache() to invalidate resolver cache ([51e4bc3](https://github.com/eric8810/catcher/commit/51e4bc30fb290d6c3ab730cf628d6a497258d971))
* **catcher-dns:** rebuild resolver on network change to pick up new DNS servers ([52f057e](https://github.com/eric8810/catcher/commit/52f057e38fc3a5fb52b470477bd39b045133c89f))


### Bug Fixes

* **catcher-dns:** prevent stale-network results repopulating cache after clear_cache ([ae35476](https://github.com/eric8810/catcher/commit/ae35476f011dc06700aee60f03c26e60aa06c5c9))
* PR [#13](https://github.com/eric8810/catcher/issues/13)–15 review findings + release 0.3.13 (issues [#28](https://github.com/eric8810/catcher/issues/28)–34) ([0320595](https://github.com/eric8810/catcher/commit/032059537d829d8535df7b0326c98c7c33ccc1a7))
* support explicit proxy for mobile clients ([a45368d](https://github.com/eric8810/catcher/commit/a45368d99f5045ececa13e0e18430919b53c38d1))
* support proxy dns behavior across http and ws ([d8ff3df](https://github.com/eric8810/catcher/commit/d8ff3df955a42fb37371e8c4714125d2af845897))
* support proxy DNS behavior across HTTP and WS ([15b1233](https://github.com/eric8810/catcher/commit/15b12333e5a233fef4ed1419c498f85f2dfb2af2))
* **ws:** share dns resolver and retry handshakes ([b16471a](https://github.com/eric8810/catcher/commit/b16471af60e57c66af5d32042478caabd19a04c2))
