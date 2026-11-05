# Acode

桌面助手里，一次会话如何把工具交给模型、在计划被批准前拦住写入，以及模型不再调用工具时这一轮如何结束。

## Language

**Plan gate**:
会话上的一道门：打开时，写入工具不能执行，直到用户批准计划。它和作曲栏上显示的模式是两回事，门开着时作曲栏仍可以显示 Agent。
_Avoid_: Ask mode, approval tier, image mode

**Planning offer**:
Plan gate 打开时，一个工具名能不能被向模型提起。已经挂在这一轮上的工具集不因此改变。
_Avoid_: schema, allowlist

**Planning run**:
Plan gate 打开时，一个工具名能不能执行。
_Avoid_: authorize, permission

**Plan-gate refusal**:
Planning run 被拒绝的那一种结果。模型看得到一句说明；循环和 Companion 靠结果本身认出这是撞门。
_Avoid_: error text, allowlist

**Turn end**:
模型没有再调用工具时，这一轮是就此结束，还是被追问后继续。用户暂停是其中一种结束。
_Avoid_: TurnComplete, completion gate, verification queue
