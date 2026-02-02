//! Integration tests for the LLM client.
//!
//! These tests make real API calls to OpenRouter.
//! Run with: OPENROUTER_API_KEY=your_key cargo test --test llm_integration -- --ignored

use synth_bench::llm::litellm::{GenerationRequest, LiteLlmClient, LlmProvider, Message};

fn get_test_api_key() -> String {
    std::env::var("OPENROUTER_API_KEY")
        .expect("OPENROUTER_API_KEY environment variable must be set for integration tests")
}

fn create_test_client() -> LiteLlmClient {
    LiteLlmClient::new_with_defaults(get_test_api_key())
}

#[tokio::test]
#[ignore] // Run with: cargo test --test llm_integration -- --ignored
async fn test_simple_generation() {
    let client = create_test_client();

    let request = GenerationRequest::new(
        "anthropic/claude-opus-4.5",
        vec![
            Message::system("You are a helpful assistant. Reply concisely."),
            Message::user("What is 2 + 2? Reply with just the number."),
        ],
    )
    .with_max_tokens(10)
    .with_temperature(0.0);

    let response = client.generate(request).await;
    assert!(response.is_ok(), "Generation failed: {:?}", response.err());

    let response = response.expect("Should have response");
    assert!(
        !response.choices.is_empty(),
        "Should have at least one choice"
    );

    let content = response.first_content().expect("Should have content");
    assert!(
        content.contains('4'),
        "Response should contain '4', got: {}",
        content
    );

    // Verify usage was tracked
    assert!(response.usage.total_tokens > 0, "Should have token usage");
}

#[tokio::test]
#[ignore]
async fn test_multi_turn_conversation() {
    let client = create_test_client();

    let request = GenerationRequest::new(
        "anthropic/claude-opus-4.5",
        vec![
            Message::system("You are a math tutor. Be concise."),
            Message::user("Remember the number 42."),
            Message::assistant("I'll remember 42."),
            Message::user("What number did I ask you to remember?"),
        ],
    )
    .with_max_tokens(20)
    .with_temperature(0.0);

    let response = client
        .generate(request)
        .await
        .expect("Generation should succeed");
    let content = response.first_content().expect("Should have content");

    assert!(
        content.contains("42"),
        "Response should mention 42, got: {}",
        content
    );
}

#[tokio::test]
#[ignore]
async fn test_template_assistant_generation() {
    use synth_bench::llm::litellm::TemplateAssistant;

    let client = create_test_client();
    let assistant = TemplateAssistant::new(Box::new(client));

    let result = assistant
        .generate_template_draft(
            "debugging",
            "log-analysis",
            "medium",
            &["grep".to_string(), "awk".to_string()],
        )
        .await;

    assert!(
        result.is_ok(),
        "Template generation failed: {:?}",
        result.err()
    );

    let draft = result.expect("Should have draft");
    // The LLM should generate YAML-like content
    assert!(!draft.is_empty(), "Draft should not be empty");
}

#[tokio::test]
#[ignore]
async fn test_instruction_improvement() {
    use synth_bench::llm::litellm::TemplateAssistant;

    let client = create_test_client();
    let assistant = TemplateAssistant::new(Box::new(client));

    let current = "Find error in log";
    let feedback = "Make the instruction clearer about what kind of error to find";

    let result = assistant.improve_instruction(current, feedback).await;

    assert!(
        result.is_ok(),
        "Instruction improvement failed: {:?}",
        result.err()
    );

    let improved = result.expect("Should have improved instruction");
    assert!(
        !improved.is_empty(),
        "Improved instruction should not be empty"
    );
    assert!(
        improved.len() > current.len(),
        "Improved instruction should be longer"
    );
}

#[tokio::test]
#[ignore]
async fn test_generation_with_temperature() {
    let client = create_test_client();

    // Test with high temperature
    let request = GenerationRequest::new(
        "anthropic/claude-opus-4.5",
        vec![Message::user("Say hello in a creative way.")],
    )
    .with_temperature(1.5)
    .with_max_tokens(50);

    let response = client.generate(request).await;
    assert!(
        response.is_ok(),
        "High temperature generation failed: {:?}",
        response.err()
    );

    let content = response
        .expect("Should have response")
        .first_content()
        .expect("Should have content")
        .to_string();
    assert!(!content.is_empty(), "Response should not be empty");
}

#[tokio::test]
async fn test_invalid_api_key() {
    let client = LiteLlmClient::new_with_defaults("invalid-key".to_string());

    let request = GenerationRequest::new("anthropic/claude-opus-4.5", vec![Message::user("test")])
        .with_max_tokens(5);

    let response = client.generate(request).await;
    assert!(response.is_err(), "Should fail with invalid API key");
}

#[tokio::test]
#[ignore]
async fn test_default_model_used() {
    let client = create_test_client();

    // Request with empty model - should use default
    let request = GenerationRequest::new("", vec![Message::user("Say 'test' and nothing else.")])
        .with_max_tokens(10);

    let response = client.generate(request).await;
    assert!(
        response.is_ok(),
        "Generation with default model failed: {:?}",
        response.err()
    );
}
