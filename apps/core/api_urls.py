from django.urls import path
from apps.reservations.api.views import ReservationViewSet
from apps.payments.api.views import PaymentViewSet
from apps.stays.api.views import StayViewSet
from apps.reports.api.views import RevenueReportView
urlpatterns=[]
from rest_framework.routers import DefaultRouter
router=DefaultRouter(); router.register('reservations',ReservationViewSet,basename='reservation'); router.register('payments',PaymentViewSet,basename='payment'); router.register('stays',StayViewSet,basename='stay')
urlpatterns += router.urls + [path('reports/revenue/', RevenueReportView.as_view())]
