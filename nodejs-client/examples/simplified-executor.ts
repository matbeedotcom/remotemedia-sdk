/**
 * Simplified Executor Example
 * 
 * Demonstrates the new simplified interface for executing remote nodes.
 */

import { withRemoteExecutor, NodeType } from '../src';

async function simplifiedSentimentAnalysis() {
  const reviews = [
    "This product is absolutely amazing! Best purchase I've ever made.",
    "Terrible quality, broke after one day. Very disappointed.",
    "It's okay, nothing special but does the job.",
    "Outstanding service and fast delivery. Highly recommend!",
    "Not worth the price. Found better alternatives elsewhere."
  ];

  await withRemoteExecutor(
    { host: 'localhost', port: 50052 },
    async (execute) => {
      console.log('🎭 Simplified Sentiment Analysis Example\n');

      // Analyze each review with the simplified interface
      for (const review of reviews) {
        const results = await execute(
          NodeType.TransformersPipelineNode,
          {
            task: 'sentiment-analysis',
            model: 'distilbert-base-uncased-finetuned-sst-2-english'
          },
          review
        );

        const [result] = results;
        const emoji = result.label === 'POSITIVE' ? '😊' : '😞';
        const percentage = (result.score * 100).toFixed(1);

        console.log(`${emoji} ${result.label} (${percentage}% confidence)`);
        console.log(`   "${review}"\n`);
      }

      // Batch analysis example
      console.log('📊 Batch Analysis with Promise.all:');
      const allResults = await Promise.all(
        reviews.map(review =>
          execute(
            NodeType.TransformersPipelineNode,
            {
              task: 'sentiment-analysis',
              model: 'distilbert-base-uncased-finetuned-sst-2-english'
            },
            review
          )
        )
      );

      const positive = allResults.filter(([r]) => r.label === 'POSITIVE').length;
      const negative = allResults.filter(([r]) => r.label === 'NEGATIVE').length;

      console.log(`  Positive reviews: ${positive}/${reviews.length}`);
      console.log(`  Negative reviews: ${negative}/${reviews.length}`);
    }
  );
}

async function multiModelComparison() {
  const text = "I love this new design! It's fantastic.";

  await withRemoteExecutor(
    { host: 'localhost', port: 50052 },
    async (execute) => {
      console.log('\n🔬 Multi-Model Comparison\n');
      console.log(`Analyzing: "${text}"\n`);

      const models = [
        'distilbert-base-uncased-finetuned-sst-2-english',
        'cardiffnlp/twitter-roberta-base-sentiment-latest',
        'nlptown/bert-base-multilingual-uncased-sentiment'
      ];

      // Execute multiple models in parallel
      const results = await Promise.all(
        models.map(async (model) => {
          try {
            const result = await execute(
              NodeType.TransformersPipelineNode,
              {
                task: 'sentiment-analysis',
                model: model,
                device: -1 // Use CPU
              },
              text
            );
            return { model, result: result[0] };
          } catch (error) {
            return { model, error: (error as Error).message };
          }
        })
      );

      // Display results
      for (const { model, result, error } of results) {
        console.log(`Model: ${model}`);
        if (error) {
          console.log(`  ❌ Error: ${error}\n`);
        } else if (result) {
          const emoji = result.label.includes('POSITIVE') || result.label.includes('POS') ? '😊' : '😞';
          const percentage = (result.score * 100).toFixed(1);
          console.log(`  ${emoji} ${result.label} (${percentage}% confidence)\n`);
        }
      }
    }
  );
}

async function calculatorExample() {
  await withRemoteExecutor(
    { host: 'localhost', port: 50052 },
    async (execute) => {
      console.log('\n🧮 Calculator Example\n');

      const operations = [
        { operation: 'add', args: [10, 5] },
        { operation: 'multiply', args: [7, 8] },
        { operation: 'subtract', args: [20, 3] },
        { operation: 'divide', args: [100, 4] }
      ];

      for (const op of operations) {
        const result = await execute(
          NodeType.CalculatorNode,
          {}, // Empty config - calculator doesn't need configuration
          {
            operation: op.operation,
            args: op.args
          }
        );

        console.log(`${op.args[0]} ${op.operation} ${op.args[1]} = ${result.result}`);
      }
    }
  );
}

// Run examples
async function main() {
  try {
    await simplifiedSentimentAnalysis();
    await multiModelComparison();
    await calculatorExample();
  } catch (error) {
    console.error('Error:', error);
  }
}

if (require.main === module) {
  main();
}