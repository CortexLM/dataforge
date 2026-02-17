const assert = require('assert');
const fs = require('fs');
const vm = require('vm');
const path = require('path');

const baseDir = path.resolve(process.cwd(), 'repo', 'js', '自动购买每天&3天&每周刷新商品');
const mainCode = fs.readFileSync(path.join(baseDir, 'main.js'), 'utf8');
const preIIFE = mainCode.split('(async function ()')[0];

function createContext(settings) {
  const context = {
    settings,
    log: { info: () => {}, warn: () => {}, error: () => {}, debug: () => {} },
    file: {
      readText: async (filePath) => fs.promises.readFile(path.join(baseDir, filePath), 'utf8'),
      ReadImageMatSync: () => { throw new Error('not needed'); }
    },
    RecognitionObject: { TemplateMatch: () => ({}) },
    sleep: async () => {},
    pathingScript: { runFile: async () => {} },
    setGameMetrics: () => {},
    click: () => {},
    captureGameRegion: () => ({ FindMulti: () => [], dispose: () => {} })
  };
  vm.createContext(context);
  vm.runInContext(preIIFE, context);
  return context;
}

async function initAndGet(context, expr) {
  await context.loadExternalData();
  await context.initNpcData([]);
  return vm.runInContext(expr, context);
}

(async () => {
  // Disable by region tag (蒙德城) should disable 石榴 but not 阿山婆.
  const ctxRegion = createContext({
    foodsToBuy: '冒险家金杯 霄灯',
    disabledTags: '蒙德城'
  });
  const regionResult = await initAndGet(
    ctxRegion,
    '({stone: npcData["石榴"].enable, ashan: npcData["阿山婆"].enable})'
  );
  assert.strictEqual(regionResult.stone, false, 'Stone should be disabled by region tag');
  assert.strictEqual(regionResult.ashan, true, 'Ashanpo should remain enabled');

  // Disable by merchant tag (黑心商人) should disable 皮托 but not 克罗丽丝.
  const ctxMerchant = createContext({
    foodsToBuy: '牛奶 金鱼草',
    disabledTags: '黑心商人'
  });
  const merchantResult = await initAndGet(
    ctxMerchant,
    '({pito: npcData["皮托"].enable, clor: npcData["克罗丽丝"].enable})'
  );
  assert.strictEqual(merchantResult.pito, false, 'Pito should be disabled by merchant tag');
  assert.strictEqual(merchantResult.clor, true, 'Clorisse should remain enabled');

  // Disable by country tag (蒙德) should disable 石榴 but not 阿山婆.
  const ctxCountry = createContext({
    foodsToBuy: '冒险家金杯 霄灯',
    disabledTags: '蒙德'
  });
  const countryResult = await initAndGet(
    ctxCountry,
    '({stone: npcData["石榴"].enable, ashan: npcData["阿山婆"].enable})'
  );
  assert.strictEqual(countryResult.stone, false, 'Stone should be disabled by country tag');
  assert.strictEqual(countryResult.ashan, true, 'Ashanpo should remain enabled');

  // Disable by dog-food merchant tag should disable 石榴 but not 阿山婆.
  const ctxDogFood = createContext({
    foodsToBuy: '冒险家金杯 霄灯',
    disabledTags: '狗粮商人'
  });
  const dogFoodResult = await initAndGet(
    ctxDogFood,
    '({stone: npcData["石榴"].enable, ashan: npcData["阿山婆"].enable})'
  );
  assert.strictEqual(dogFoodResult.stone, false, 'Stone should be disabled by dog-food tag');
  assert.strictEqual(dogFoodResult.ashan, true, 'Ashanpo should remain enabled');

  console.log('disabled_tags tests passed');
})();
