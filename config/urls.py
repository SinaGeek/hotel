from django.contrib import admin
from django.urls import path, include
from rest_framework_simplejwt.views import TokenObtainPairView, TokenRefreshView
from drf_spectacular.views import SpectacularAPIView, SpectacularSwaggerView
from apps.core.views import dashboard_view, set_theme

urlpatterns = [
    path('i18n/', include('django.conf.urls.i18n')),
    path('', dashboard_view, name='dashboard'),
    path('accounts/', include('apps.accounts.urls')),
    path('rooms/', include('apps.rooms.urls')),
    path('guests/', include('apps.guests.urls')),
    path('housekeeping/', include('apps.housekeeping.urls')),
    path('inventory/', include('apps.inventory.urls')),
    path('admin/', admin.site.urls),
    path('api/token/', TokenObtainPairView.as_view()),
    path('api/token/refresh/', TokenRefreshView.as_view()),
    path('api/schema/', SpectacularAPIView.as_view()),
    path('api/docs/', SpectacularSwaggerView.as_view(url_name='schema')),
    path('api/', include('apps.core.api_urls')),
    path('api/core/set-theme/', set_theme, name='set_theme'),
]

from django.conf import settings
from django.conf.urls.static import static

if settings.DEBUG:
    urlpatterns += static(settings.MEDIA_URL, document_root=settings.MEDIA_ROOT)
