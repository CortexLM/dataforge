const assert = require('assert')
const fs = require('fs')
const path = require('path')
const Module = require('module')
const React = require('react')

// Patch React internals for react-test-renderer compatibility
if (!React.__SECRET_INTERNALS_DO_NOT_USE_OR_YOU_WILL_BE_FIRED) {
  React.__SECRET_INTERNALS_DO_NOT_USE_OR_YOU_WILL_BE_FIRED =
    React.__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE
}

const TestRenderer = require('react-test-renderer')
const { act } = TestRenderer
const babel = require('@babel/core')

const targetPath = path.join(
  __dirname,
  '..',
  'src',
  'singlevendor',
  'screens',
  'FAQS',
  'FAQS.js'
)

assert.ok(fs.existsSync(targetPath), 'Expected FAQS component to exist')

const themeProxy = new Proxy(
  {
    fontMainColor: '#111',
    fontSecondColor: '#222'
  },
  {
    get: (target, prop) => (prop in target ? target[prop] : '#000')
  }
)

const originalLoad = Module._load

Module._load = function (request, parent, isMain) {
  if (request === 'react') {
    return React
  }
  if (request === 'react-native') {
    const createElement = (name) => {
      const Comp = (props) => React.createElement(name, props, props.children)
      Comp.displayName = name
      return Comp
    }
    class AnimatedValue {
      constructor(value) {
        this.value = value
      }
      interpolate() {
        return '0deg'
      }
    }
    return {
      SafeAreaView: createElement('SafeAreaView'),
      ScrollView: createElement('ScrollView'),
      View: createElement('View'),
      TouchableOpacity: createElement('TouchableOpacity'),
      Animated: {
        Value: AnimatedValue,
        spring: () => ({ start: () => {} })
      }
    }
  }
  if (request === '@react-navigation/native') {
    return { useNavigation: () => ({ goBack: () => {} }) }
  }
  if (request === 'react-i18next') {
    return {
      useTranslation: () => ({ t: (value) => value, i18n: { dir: () => 'ltr' } })
    }
  }
  if (request === '@expo/vector-icons') {
    const Icon = (props) => React.createElement('Icon', props, props.children)
    return { Ionicons: Icon, MaterialCommunityIcons: Icon }
  }
  if (request === '../../../ui/ThemeContext/ThemeContext') {
    return React.createContext({ ThemeValue: 'Pink' })
  }
  if (request === '../../../utils/themeColors') {
    return { theme: { Pink: themeProxy } }
  }
  if (request === '../../components/AccountSectionHeader') {
    return (props) => React.createElement('AccountSectionHeader', props)
  }
  if (request === '../../../components/Text/TextDefault/TextDefault') {
    return (props) => React.createElement('TextDefault', props, props.children)
  }
  if (request === './styles') {
    return () => new Proxy({}, { get: () => ({}) })
  }
  return originalLoad(request, parent, isMain)
}

const source = fs.readFileSync(targetPath, 'utf8')
const transformed = babel.transformSync(source, {
  presets: ['babel-preset-expo'],
  plugins: ['@babel/plugin-transform-modules-commonjs'],
  filename: targetPath
})

const compiledModule = new Module(targetPath, module)
compiledModule._compile(transformed.code, targetPath)
const FAQS = compiledModule.exports.default

assert.ok(typeof FAQS === 'function', 'FAQS component should be a function')

const ThemeContext = require('../../../enatega-multivendor-app/src/ui/ThemeContext/ThemeContext')

let renderer
act(() => {
  renderer = TestRenderer.create(
    React.createElement(
      ThemeContext.Provider,
      { value: { ThemeValue: 'Pink' } },
      React.createElement(FAQS)
    )
  )
})

const initialTextNodes = renderer.root.findAll(
  (node) => node.type === 'TextDefault'
)
const touchables = renderer.root.findAll(
  (node) => node.type === 'TouchableOpacity'
)
assert.ok(touchables.length > 0, 'Expected at least one accordion header')

act(() => {
  touchables[0].props.onPress()
})

const expandedTextNodes = renderer.root.findAll(
  (node) => node.type === 'TextDefault'
)
assert.ok(
  expandedTextNodes.length > initialTextNodes.length,
  'Accordion should expand to show more text'
)

act(() => {
  touchables[0].props.onPress()
})

const collapsedTextNodes = renderer.root.findAll(
  (node) => node.type === 'TextDefault'
)
assert.strictEqual(
  collapsedTextNodes.length,
  initialTextNodes.length,
  'Accordion should collapse back to original text count'
)

Module._load = originalLoad

console.log('FAQS accordion tests passed')
