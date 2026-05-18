# D 系 — 图表规则

本文件定义 thesis skill 生成图表（图、三线表、drawio 文件）的规则。规则编号 D.x 与 `rules-index.md` 一致。

> **范围说明**：本文件**不含字号/字体硬编码**——这些数据从用户提供的范文/模板抓取（见 `format-rules.md` E 系）。本文件只放跨学校通用的学术普世结构规则。

---

## D.1 图表类型选择

| 内容类型 | 推荐形式 |
|---------|---------|
| 数据对比（≥2 项） | 三线表 / 柱状图 |
| 流程 / 工作流 | 流程图 |
| 关系 / 层级 | 结构图 / 概念图 |
| 时间趋势 | 折线图 |
| 占比 / 构成 | 饼图（少用） |
| 模型 / 理论框架 | 结构示意图 |
| 调研 / 实验数据 | 三线表 |
| 分类 | 三线表 |

### 提议工作流
1. 在写作中遇到适合用图/表的位置，**暂停**，向用户提议（类型 + 内容 + 为何强化此处论证）
2. **等用户确认**后再生成
3. 生成时按下方对应规则

---

## D.2 三线表（学术普世硬规则）

中文学术论文使用三线表（Three-Line Table）。本节规则跨学校通用，不依赖具体字号字体。

```
                    表X 表格标题（居中，表上方）
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  ← 顶线（粗）
  列标题1        列标题2        列标题3
─────────────────────────────────────────  ← 栏目线（细）
  数据            数据            数据
  数据            数据            数据
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  ← 底线（粗）
注：说明文字（如有）
数据来源：XXX（如有）
```

### 硬规则
- D.2.1 标题位置：表**上方**居中（hook 不检查，但 G 系将来扩展）
- D.2.2 标题末**无标点**
- D.2.3 编号顺序：表1, 表2, 表3...，按论文顺序递增
- D.2.4 **三线**：仅保留顶线、栏目线、底线，**禁止竖线**，**禁止内部其他横线**
- D.2.5 单位写在列标题（如"温度/°C"），不写在数据格
- D.2.6 数字右对齐 + 小数点对齐；文本左对齐
- D.2.7 表格本身居中
- D.2.8 注释 / 数据来源在表下方，分别以"注："和"数据来源："开头

### 字号字体
**不在本文件硬编码**。从 Phase 0 范文/模板抓取（见 E 系 E.3）。

---

## D.3 图（学术普世硬规则）

### 硬规则
- D.3.1 标题位置：图**下方**居中
- D.3.2 标题末**无标点**
- D.3.3 编号顺序：图1, 图2...，按论文顺序递增
- D.3.4 图本身居中
- D.3.5 主线 > 辅线（坐标线粗细差异）
- D.3.6 标值线（坐标刻度）在图**内侧**
- D.3.7 中文标签：所有坐标轴、图例、标题用中文（论文是中文时）
- D.3.8 节点/形状底色：白色 `#FFFFFF` 强制
- D.3.9 节点边框：黑色 `#000000` 强制
- D.3.10 颜色基调：学术克制，避免高饱和度强对比色
- D.3.11 数据来源：在标题下方注明（如有外部来源）

### 字号字体
**不在本文件硬编码**。从范文/模板抓取。

---

## D.4 用例图（UML 简化学术惯例）

中国本科毕业论文用例图遵循**简化学术约定**，不是完整 UML 规范：

- D.4.1 角色：单个 `umlActor` stick 图，**左侧**，名称在下方（如"用户"、"管理员"）。垂直居中对齐用例列
- D.4.2 用例：椭圆，单**纵向列**排在角色右侧，按论文文中出现顺序自上而下
- D.4.3 统一尺寸：所有用例宽高一致（如 140×50），垂直步长一致（如 60px）
- D.4.4 **禁止系统边界矩形**：不画"系统名"外框；中国本科评审接受简化，加框反而过密
- D.4.5 关联线：直线 `endArrow=none`，从角色到每个用例。**禁用 `orthogonalEdgeStyle`**——直线是 UML 用例图惯例，符合"关联，非流向"语义
- D.4.6 节点风格：椭圆和角色全部 `fillColor=#FFFFFF` + `strokeColor=#000000`
- D.4.7 单图用例数 ≤12：超过则按自然维度拆分（如"用户端登录与购物用例图" + "用户端宠物养成用例图"），不挤多列

> 依据：CNKI 2022-2025 大量 FastAdmin/ThinkPHP/uni-app 本科毕业论文实证。完整 UML 加边界框、include/extend 模板对本科超规格。

---

## D.5 E-R 实体属性图（学术普世硬规则）

中国本科毕业论文 E-R 属性图遵循严格简化约定。

### 硬规则
- D.5.1 **所有标签必须中文**。禁止英文 SQL 字段名直接出现在椭圆里。映射示例：
  - `id` → `编号`
  - `username` → `用户名`
  - `user_id` → `用户编号`
  - `order_no` → `订单号`
  - `amount` → `订单金额`
  - `pay_method` → `支付方式`
  - `createtime` → `创建时间`
  - `openid` → `微信标识`
  - `unionid` → `用户唯一标识`
  
  > 概念设计（E-R）和物理设计（数据库表三线表）必须分层：英文 SQL 字段属于 4.3.2 数据库表设计，不能混入概念图

- D.5.2 实体（中心）：矩形，白底，加粗文字。形状：`rounded=0;whiteSpace=wrap;html=1;fontStyle=1;`（fontStyle=1 是粗体位）

- D.5.3 属性（周围）：椭圆，白底，常规文字。形状：`ellipse;whiteSpace=wrap;html=1;`

- D.5.4 主键标记：椭圆 + `fontStyle=4`（drawio bitmask: 1=bold, 2=italic, 4=underline；按位或组合）+ `strokeWidth=2` 加粗边框。**禁用 HTML `<u>` 标签**——drawio 跨版本渲染不可靠，使用原生 fontStyle

- D.5.5 **不加外键标记**。不要在属性后写 `(FK)`。外键是物理实现层概念，E-R 只表达关联存在

- D.5.6 连接：直线 `edgeStyle=none;rounded=0;endArrow=none`。**禁用 `orthogonalEdgeStyle`**——E-R 关联是概念绑定，非流向

- D.5.7 布局：环形分布。简单算法：`(cx + R·cosθ, cy + R·sinθ)`，θ 在 [-90°, 270°) 等距，R≈250-300，cx/cy 居中

- D.5.8 **一个图一个实体**。多实体关系图用单独"实体联系图"（chen 表示法），不挤同一图

### 已知错误（明令禁止）
- 椭圆里写 `id, username, openid`
- 用 `<u>...</u>` 实现下划线
- 属性后追加 `(FK)`
- 用 `orthogonalEdgeStyle` 连接

---

## D.6 流程图（学术惯例）

- D.6.1 连接线：仅**直角**正交线，禁止曲线和斜线
- D.6.2 形状语义：
  - 矩形 → 处理 / 动作步骤
  - 菱形 → 判断 / 决策（标"是/否"或"Y/N"）
  - 圆角矩形 → 起止节点
  - 平行四边形 → 输入 / 输出
- D.6.3 箭头：实线，箭头指示流向
- D.6.4 布局：主流自上而下；分支自左到右
- D.6.5 分支必须**标签**：判断节点的两侧线必须标"是"/"否"
- D.6.6 形状内文字：简明，中文
- D.6.7 形状间间距统一

---

## D.7 drawio 文件输出（强制）

所有图表（流程图、结构图、E-R、用例图、数据图）**必须输出为 `.drawio` XML 文件**。

### D.7.1 文件命名
格式：`图[编号]_[中文描述].drawio`
- 正例：`图1_系统架构.drawio`、`图3.1_用户管理用例图.drawio`
- 反例（禁用英文）：`fig1_architecture.drawio`

### D.7.2 模板

```xml
<mxfile host="app.diagrams.net" modified="<日期>" agent="thesis-skill" version="24.0">
  <diagram name="图1" id="unique-id">
    <mxGraphModel dx="1024" dy="768" grid="1" gridSize="10" guides="1" tooltips="1" connect="1" arrows="1" fold="1" page="1" pageScale="1" pageWidth="827" pageHeight="1169" math="0" shadow="0">
      <root>
        <mxCell id="0" />
        <mxCell id="1" parent="0" />
        <mxCell id="2" value="节点文字" style="rounded=0;whiteSpace=wrap;html=1;fillColor=#FFFFFF;strokeColor=#000000;" vertex="1" parent="1">
          <mxGeometry x="100" y="100" width="120" height="40" as="geometry" />
        </mxCell>
        <mxCell id="3" value="判断条件" style="rhombus;whiteSpace=wrap;html=1;fillColor=#FFFFFF;strokeColor=#000000;" vertex="1" parent="1">
          <mxGeometry x="90" y="180" width="140" height="60" as="geometry" />
        </mxCell>
        <mxCell id="4" style="edgeStyle=orthogonalEdgeStyle;rounded=0;" edge="1" source="2" target="3" parent="1">
          <mxGeometry relative="1" as="geometry" />
        </mxCell>
      </root>
    </mxGraphModel>
  </diagram>
</mxfile>
```

### D.7.3 关键约束
- **绝对禁止 XML 注释 `<!-- -->`**——会让 draw.io 解析报错
- 所有节点 `fillColor=#FFFFFF`
- 所有连线 `edgeStyle=orthogonalEdgeStyle`（**例外**：用例图 D.4.5 / E-R 图 D.5.6 用直线）
- 边框 `strokeColor=#000000`
- 矩形：处理用 `rounded=0`，起止用 `rounded=1`
- 菱形：`rhombus`
- 页面 A4：`827x1169`

### D.7.4 页面尺寸单位
A4 = 827×1169（draw.io px 单位，对应 21cm × 29.7cm）

---

## D.8 图表正文整合

- D.8.1 **先文后图**：图表**必须**在正文中先被引用再出现。如 "如表1所示，..."、"图2 展示了..."、"实验结果见表3"
- D.8.2 必须有分析：每个图表后必须有**分析文字**——禁止只插图不解读
- D.8.3 就近放置：图表应紧邻其首次引用位置
- D.8.4 跨节再引：在后续章节再次讨论同一图表时显式重引（如"根据表1中的数据..."）

---

## 常见错误

- 插图不分析（只显示，无解读）
- 标题位置错（表题写下方 / 图题写上方）
- 三线表加竖线或额外横线
- 编号断裂或乱序
- 中文论文里出现英文坐标轴 / 图例
- 用文字复述图表已展示的数据（应分析而非复述）

---

## D.9 Python-docx 自动化生成表格的硬规则

D.9 系列适用于使用 python-docx 通过 OxmlElement 自构表格的所有自动化场景（修正模式、批量生成、模板应用等）。**不遵守 D.9.1 必然导致 cell 内容看起来"前面被加了 2 个空字符"——即使代码里 `<w:jc w:val="center"/>` 设置正确**，因为 cell 段落会隐式继承文档默认 Normal 样式的 `<w:ind w:firstLineChars="200">`（首行缩进 2 字符），渲染等同 leading 空字符填充，是论文表格的死罪。

> 教训来源：2026-05-08 校园智能问答系统论文修正会话。在 4.1 节插入子节"4.1.1 RAG-Prompt 工程规范"时，用 python-docx 生成的两张三线表被用户指出"cell 文本前看着有 2 个空字符，按退格键能删 2 字符"。诊断后发现根因是 cell 段落隐式继承 Normal 样式的 `firstLineChars=200`。

### D.9.1 cell 段落必须显式清零缩进（HARD-GATE）

**触发场景**：在 `<w:tc>` 内创建 `<w:p>` 写表格内容。

**症状**：cell 文本左侧出现约 2 字符宽的空白，按退格键能"删 2 字符"才到文字开头；jc=center 看起来像"假居中"，jc=left 看起来像"前面打了 2 个空格"。

**根因**：python-docx 的 `doc.add_table()` 创建的 cell 默认段落样式继承自文档 Normal 样式。中文论文模板（含本科论文模板）通常在 Normal 上设有 `<w:ind w:firstLineChars="200"/>`（正文首行缩进 2 字符）——这个属性会原样下沉到 cell 内段落。除非 cell 段落自己写了 `<w:ind firstLineChars="0">`，Word 会按 Normal 的 200 渲染。

**反模式（一律禁止）**：
- `cell_text = '   ' + 内容` —— 用半角/全角/NBSP 空字符做"伪居中"补偿
- `cell_text = 内容.center(N)` —— 用 Python 的 str.center 加左右 padding
- 任何在文本字面里加 leading whitespace 的做法（半角空格、全角空格、NBSP、em space、zero-width space）

**正确做法**：cell 段落的 `<w:pPr>` 中显式写 `<w:ind>` 清零，再设 `<w:jc>`：

```python
from docx.oxml import OxmlElement
from docx.oxml.ns import qn

def make_cell_paragraph(cell, text, *, align='center'):
    """正确写表格 cell 段落：清空默认段落 → 清零缩进 → 设对齐 → 写 run。"""
    # 1. 清空 cell 自带的默认段落
    for child in list(cell._tc.findall(qn('w:p'))):
        cell._tc.remove(child)

    # 2. 新建段落并显式清零缩进（关键步骤，遗漏即触发 D.9.1 故障）
    p = OxmlElement('w:p')
    pPr = OxmlElement('w:pPr')
    ind = OxmlElement('w:ind')
    for attr in ('firstLineChars', 'firstLine', 'leftChars', 'left',
                 'rightChars', 'right'):
        ind.set(qn(f'w:{attr}'), '0')
    pPr.append(ind)

    # 3. 段落对齐
    jc = OxmlElement('w:jc')
    jc.set(qn('w:val'), align)
    pPr.append(jc)

    p.append(pPr)
    # 4. 写 run（rFonts / color / size / 文本）
    cell._tc.append(p)
```

**自检（写完表格后必须扫一遍）**：

```python
def assert_no_inherited_indent(table):
    """每个 cell 段落都必须有 <w:ind> 且 4 个关键属性全 '0'。否则触发 D.9.1。"""
    for row in table.rows:
        for cell in row.cells:
            for p in cell.paragraphs:
                pPr = p._p.find(qn('w:pPr'))
                assert pPr is not None, 'cell paragraph 无 pPr'
                ind = pPr.find(qn('w:ind'))
                assert ind is not None, 'cell paragraph 无 <w:ind>，将继承 Normal 缩进'
                for attr in ('firstLineChars', 'firstLine', 'leftChars', 'left'):
                    v = ind.get(qn(f'w:{attr}'))
                    assert v == '0', f'<w:ind {attr}={v!r}> 未清零'
```

### D.9.2 cell 文本字面禁含 leading/trailing 空字符（HARD-GATE）

无论 D.9.1 是否生效，cell 文本字面（即写入 `<w:t>` 的字符串）必须 `lstrip` + `rstrip` 处理空白字符簇。这是一道独立兜底——即便 D.9.1 失效，至少不会留下 leading whitespace。

**禁止字符簇**：
- 半角空格（U+0020）、tab（U+0009）
- 全角空格（U+3000）
- NBSP（U+00A0）
- em space（U+2003）
- zero-width space（U+200B）
- BOM / zero-width no-break space（U+FEFF）

**自检**：

```python
WHITESPACE_TO_TRIM = ''.join([
    ' ', '\t',
    '　',           # 全角空格
    ' ',           # NBSP
    ' ',           # em space
    '​',           # zero-width space
    '﻿',           # BOM
])

def assert_no_leading_whitespace(table):
    for row in table.rows:
        for cell in row.cells:
            for p in cell.paragraphs:
                for r in p.runs:
                    raw = r.text
                    assert raw == raw.lstrip(WHITESPACE_TO_TRIM), \
                        f'cell run leading whitespace: {raw[:20]!r}'
                    assert raw == raw.rstrip(WHITESPACE_TO_TRIM), \
                        f'cell run trailing whitespace: {raw[:20]!r}'
```

### D.9.3 整段叙述 vs cell 段落的首行缩进双标处理

- **Normal 段落**（正文整段）允许保留 `firstLineChars=200`——这是中文论文正文段的标准 2 字符首行缩进
- **cell 内段落**绝不允许继承——cell 是表格单元格，不是正文整段，需要完全靠 jc 居中或 left 对齐

不要因为修了 cell 段落的缩进，把正文 Normal 段落也清零。两者属于不同语境，需独立判定。

### D.9.4 修正模式下的 cell 缩进排查清单

如果用户报告"cell 看起来前面有 2 个空格、按退格能删"，按以下顺序排查：

1. 读 cell 的 XML，检查 `<w:t>` 是否含 leading whitespace —— 若有，违反 D.9.2，立即修
2. 检查 cell 段落的 `<w:pPr>` 是否有 `<w:ind firstLineChars="0">` —— 若无，违反 D.9.1，立即修
3. 检查 cell 段落的 `<w:jc>` 是否设为预期对齐方式 —— 验证段落对齐属性本身是否正确写入
4. 检查表格 `<w:tblPr>` 的 `<w:tblInd>` —— 整张表的左缩进是否被错误设置（应为 0 或与表格居中对齐相协）

报告排查结果时给出 XML 片段证据，不靠"我以为"。

---

## 执行（hook 实施位置）

| 规则 | 检查方式 | 对应 G ID |
|------|---------|----------|
| D.8.1 先文后图（图表声明 vs 正文引用对齐）| 比对 outline.md 声明的图表号与 docx 正文是否含 `图X.Y` `表X.Y` 字样 | G.8 |
| D.9.1 cell 段落显式清零 `<w:ind>` | 扫每个 `<w:tc>/<w:p>/<w:pPr>/<w:ind>` 4 个属性是否全 '0' | G.23 |
| D.9.2 cell 文本无 leading/trailing 空白 | 扫每个 `<w:tc>/<w:p>/<w:r>/<w:t>` 文本字面 | G.24 |
| D.7.1 drawio 文件命名（中文）| 流程式（写文件时检查）| 流程式 |
| 其他 | 多数为流程式提议规则，hook 不机械检查 | — |
