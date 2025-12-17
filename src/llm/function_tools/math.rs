use super::FunctionTool;
use serde_json::{json, Value};

#[derive(Debug, Default)]
pub struct MathTool;

impl MathTool {
    pub fn new() -> Self { Self }
}

impl FunctionTool for MathTool {
    fn name(&self) -> &str { "math" }

    fn description(&self) -> &str {
        "Perform basic arithmetic on two numbers: add, sub, mul, div."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "a": { "type": "number", "description": "First operand" },
                "b": { "type": "number", "description": "Second operand" },
                "op": { "type": "string", "enum": ["add","sub","mul","div"], "description": "Operation to perform" }
            },
            "required": ["a","b"],
            "additionalProperties": false
        })
    }

    fn call(&self, arguments: Value) -> Result<Value, String> {
        let a = arguments.get("a").and_then(|v| v.as_f64()).ok_or_else(|| "missing number 'a'".to_string())?;
        let b = arguments.get("b").and_then(|v| v.as_f64()).ok_or_else(|| "missing number 'b'".to_string())?;
        let op = arguments.get("op").and_then(|v| v.as_str()).unwrap_or("add");

        let result = match op {
            "add" => a + b,
            "sub" => a - b,
            "mul" => a * b,
            "div" => {
                if b == 0.0 { return Err("division by zero".to_string()); }
                a / b
            },
            _ => return Err(format!("unsupported op: {}", op)),
        };

        Ok(json!({
            "a": a,
            "b": b,
            "op": op,
            "result": result
        }))
    }
}
