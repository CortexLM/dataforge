import assert from 'node:assert/strict'
import { linkCard } from '../eleventy/shortcodes/link-card.js'

// Helper to normalize whitespace for easier matching
const normalize = (html) => html.replace(/\s+/g, ' ').trim()

// Test: icon is wrapped in a container and background colour is applied
{
  const html = normalize(
    linkCard({
      title: 'Data',
      description: 'Find guidance on collecting data and using it well',
      icon: './data.svg',
      iconBackgroundColour: '#123456'
    })
  )

  assert.match(
    html,
    /class="app-link-card__icon-container"/,
    'expected icon to be wrapped in a container element'
  )
  assert.match(
    html,
    /style="--icon-background: #123456"/,
    'expected icon background colour to be set via inline style'
  )
  assert.match(
    html,
    /class="app-link-card__icon"/,
    'expected icon image to be rendered'
  )
}

// Test: icon still renders without a background colour, but no background style is set
{
  const html = normalize(
    linkCard({
      title: 'Brand in use',
      description: 'Examples of GOV.UK branding in practice',
      icon: './brand.svg'
    })
  )

  assert.match(
    html,
    /class="app-link-card__icon-container"/,
    'expected icon container to render when an icon is provided'
  )
  assert.doesNotMatch(
    html,
    /--icon-background/,
    'expected no icon background style when iconBackgroundColour is omitted'
  )
  assert.match(
    html,
    /class="app-link-card__icon"/,
    'expected icon image to be rendered without background colour'
  )
}

console.log('link-card shortcode tests passed')
