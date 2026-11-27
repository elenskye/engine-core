# 0006 简繁归一化使用 OpenCC 逐字表

2026-11-27，站主决定。

随库带入 OpenCC `ver.1.1.9` 的 `TSCharacters.txt` 和 Apache-2.0 许可证。原表 34,627 字节，共 4,113 个单字。每字沿首个简体转换值折叠到稳定值，例如「薴、苧」都折到「苎」。查询与原文用同一映射，转换前后的单字位置一一对应。

`ferrous-opencc` 0.4.0 的本机解包约 3.2 MiB，带短语转换、多个方向的字典和额外依赖。单字查找选择只使用逐字表，语境例外和地区词汇继续按逐字结果比较。 [OpenCC 原表](https://github.com/BYVoid/OpenCC/blob/ver.1.1.9/data/dictionary/TSCharacters.txt) · [ferrous-opencc](https://github.com/apoint123/ferrous-opencc)
