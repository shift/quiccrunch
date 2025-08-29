use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Operation {
    Add(f64, f64),
    Subtract(f64, f64),
    Multiply(f64, f64),
    Divide(f64, f64),
    Power(f64, f64),
    Sqrt(f64),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CalculatorRequest {
    pub operation: Operation,
    pub id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CalculatorResponse {
    pub result: Result<f64, String>,
    pub id: String,
}

impl CalculatorRequest {
    pub fn new(operation: Operation) -> Self {
        Self {
            operation,
            id: uuid::Uuid::new_v4().to_string(),
        }
    }
    
    pub fn calculate(&self) -> CalculatorResponse {
        let result = match &self.operation {
            Operation::Add(a, b) => Ok(a + b),
            Operation::Subtract(a, b) => Ok(a - b),
            Operation::Multiply(a, b) => Ok(a * b),
            Operation::Divide(a, b) => {
                if *b == 0.0 {
                    Err("Division by zero".to_string())
                } else {
                    Ok(a / b)
                }
            }
            Operation::Power(a, b) => Ok(a.powf(*b)),
            Operation::Sqrt(a) => {
                if *a < 0.0 {
                    Err("Square root of negative number".to_string())
                } else {
                    Ok(a.sqrt())
                }
            }
        };
        
        CalculatorResponse {
            result,
            id: self.id.clone(),
        }
    }
}