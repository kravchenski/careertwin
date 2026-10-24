"""Agent-facing interface: the CareerTwin HTTP API exposed as tools.

`client` talks to the running API, `registry` holds the tool catalogue,
`mcp_server` serves it over MCP and `runner` drives it from any
OpenAI-compatible endpoint such as a Switchyard gateway.
"""
