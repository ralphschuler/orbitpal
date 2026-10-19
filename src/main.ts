import { mount } from 'svelte';
import App from './App.svelte';
import Overlay from './Overlay.svelte';
import './style.css';
import './i18n.css';

const overlay = window.location.pathname === '/overlay' || new URLSearchParams(location.search).has('overlay');
mount(overlay ? Overlay : App, { target: document.getElementById('app')! });
